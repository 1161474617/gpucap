//! 核心采集器模块：显卡、内存、原生进程枚举、原生系统事件流、崩溃报告监视。

use crate::infra::{boot_log, now_iso, run_hidden, to_wide};
use chrono::{DateTime, Local};
use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
use nvml_wrapper::Nvml;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::EventLog::{
    EvtClose, EvtFormatMessageEvent, EvtNext, EvtOpenPublisherMetadata, EvtQuery,
    EvtQueryChannelPath, EvtQueryReverseDirection, EvtRender, EvtRenderEventXml, EVT_HANDLE,
};

#[link(name = "wevtapi")]
extern "system" {
    fn EvtFormatMessage(
        PublisherMetadata: EVT_HANDLE,
        Event: EVT_HANDLE,
        MessageId: u32,
        ValueCount: u32,
        Values: *const std::ffi::c_void,
        Flags: u32,
        BufferSize: u32,
        Buffer: *mut u16,
        BufferUsed: *mut u32,
    ) -> i32;
}
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows_sys::Win32::System::ProcessStatus::{
    K32GetPerformanceInfo, K32GetProcessMemoryInfo, PERFORMANCE_INFORMATION,
    PROCESS_MEMORY_COUNTERS,
};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
};

/// 显卡核心查询字段
pub const GPU_CORE_FIELDS: &[&str] = &[
    "temperature.gpu",
    "power.draw",
    "power.limit",
    "clocks.current.graphics",
    "clocks.current.memory",
    "pcie.link.gen.current",
    "pcie.link.width.current",
    "utilization.gpu",
    "utilization.memory",
    "memory.used",
    "memory.total",
    "fan.speed",
    "clocks_throttle_reasons.active",
];

/// 显卡可选扩展字段
pub const GPU_EXTRA_FIELDS: &[&str] = &[
    "temperature.memory",
    "pcie.link.gen.max",
    "pcie.link.width.max",
];

/// NVAPI 热敏传感器结构体（60 字节 V1 布局，每单位为 1/256 °C）
#[repr(C)]
struct NvThermalSensors {
    version: u32,
    mask: u32,
    reserved: [u32; 8],
    temperatures: [i32; 5],
}

type NvApiQueryInterfaceFn = unsafe extern "C" fn(u32) -> *const std::ffi::c_void;
type NvApiInitializeFn = unsafe extern "C" fn() -> i32;
type NvApiEnumPhysicalGpusFn = unsafe extern "C" fn(*mut *mut std::ffi::c_void, *mut u32) -> i32;
type NvApiGpuGetBusIdFn = unsafe extern "C" fn(*mut std::ffi::c_void, *mut u32) -> i32;
type NvApiGpuThermalGetSensorsFn =
    unsafe extern "C" fn(*mut std::ffi::c_void, *mut NvThermalSensors) -> i32;

/// 通过 NVIDIA 驱动自带 nvapi64.dll 直采 GPU 热点温度（Hotspot）
struct NvApiHotspot {
    gpu_handle: *mut std::ffi::c_void,
    get_sensors: NvApiGpuThermalGetSensorsFn,
}

unsafe impl Send for NvApiHotspot {}

impl NvApiHotspot {
    fn new(target_bus_id: Option<u32>, fallback_idx: usize) -> Option<Self> {
        unsafe {
            let lib = LoadLibraryA(b"nvapi64.dll\0".as_ptr());
            if lib.is_null() {
                return None;
            }
            let qi_ptr = GetProcAddress(lib, b"nvapi_QueryInterface\0".as_ptr())?;
            let query_interface: NvApiQueryInterfaceFn = std::mem::transmute(qi_ptr);

            let init_ptr = query_interface(0x0150E828);
            let enum_ptr = query_interface(0xE5AC921F);
            let bus_ptr = query_interface(0x1BE0B8E5);
            let sensors_ptr = query_interface(0x65FE3AAD);
            if init_ptr.is_null() || enum_ptr.is_null() || sensors_ptr.is_null() {
                return None;
            }

            let nv_init: NvApiInitializeFn = std::mem::transmute(init_ptr);
            if nv_init() != 0 {
                return None;
            }

            let nv_enum: NvApiEnumPhysicalGpusFn = std::mem::transmute(enum_ptr);
            let mut handles = [std::ptr::null_mut(); 64];
            let mut count: u32 = 0;
            if nv_enum(handles.as_mut_ptr(), &mut count) != 0 || count == 0 {
                return None;
            }

            let mut chosen = handles.get(fallback_idx).copied().unwrap_or(handles[0]);
            if let Some(target_bus) = target_bus_id {
                if !bus_ptr.is_null() {
                    let nv_bus: NvApiGpuGetBusIdFn = std::mem::transmute(bus_ptr);
                    for &h in &handles[..count as usize] {
                        let mut bus_id: u32 = 0;
                        if nv_bus(h, &mut bus_id) == 0 && bus_id == target_bus {
                            chosen = h;
                            break;
                        }
                    }
                }
            }
            if chosen.is_null() {
                return None;
            }

            let get_sensors: NvApiGpuThermalGetSensorsFn = std::mem::transmute(sensors_ptr);
            let instance = Self {
                gpu_handle: chosen,
                get_sensors,
            };
            if instance.read_hotspot(None).is_some() {
                Some(instance)
            } else {
                None
            }
        }
    }

    fn read_hotspot(&self, core_temp_c: Option<f64>) -> Option<f64> {
        let mut sensors = NvThermalSensors {
            version: (1 << 16) | (std::mem::size_of::<NvThermalSensors>() as u32),
            mask: 0x7,
            reserved: [0; 8],
            temperatures: [0; 5],
        };
        let rc = unsafe { (self.get_sensors)(self.gpu_handle, &mut sensors) };
        if rc != 0 {
            return None;
        }
        // 通道 1（temperatures[1]）为 NVIDIA GPU Hotspot 热点传感器
        if (sensors.mask & (1 << 1)) != 0 {
            let t = sensors.temperatures[1] as f64 / 256.0;
            if (5.0..=135.0).contains(&t) {
                return Some((t * 100.0).round() / 100.0);
            }
        }
        // 兜底在有效通道中选取最高温度（不低于核心温度）
        let mut best: Option<f64> = None;
        for ch in 0..5 {
            if (sensors.mask & (1u32 << ch)) == 0 {
                continue;
            }
            let t = sensors.temperatures[ch] as f64 / 256.0;
            if !(5.0..=135.0).contains(&t) {
                continue;
            }
            if let Some(core) = core_temp_c {
                if t + 1.0 < core {
                    continue;
                }
            }
            best = Some(best.map_or(t, |cur| cur.max(t)));
        }
        best.map(|v| (v * 100.0).round() / 100.0)
    }
}

/// 显卡采集器（优先使用原生 NVML + NVAPI 热点直采，异常时自动回退 nvidia-smi.exe）
pub struct GpuSampler {
    pub exe: String,
    pub gpu_index: usize,
    pub fields: Vec<String>,
    pub extra: Vec<String>,
    nvml: Option<Nvml>,
    nvapi_hotspot: Option<NvApiHotspot>,
}

impl GpuSampler {
    pub fn new(gpu_index: usize) -> Self {
        let exe = Self::find_exe();
        let nvml = Self::init_nvml();
        let bus_id = nvml
            .as_ref()
            .and_then(|n| n.device_by_index(gpu_index as u32).ok())
            .and_then(|d| d.pci_info().ok())
            .map(|p| p.bus);
        let nvapi_hotspot = NvApiHotspot::new(bus_id, gpu_index);
        if nvapi_hotspot.is_some() {
            boot_log(&format!(
                "[info] GpuSampler #{} hotspot sensor ready via nvapi64.dll",
                gpu_index
            ));
        }
        let mut sampler = Self {
            exe,
            gpu_index,
            fields: GPU_CORE_FIELDS.iter().map(|s| s.to_string()).collect(),
            extra: Vec::new(),
            nvml,
            nvapi_hotspot,
        };
        if let Some(ref n) = sampler.nvml {
            if n.device_by_index(gpu_index as u32).is_ok() {
                sampler.fields = GPU_CORE_FIELDS
                    .iter()
                    .chain(GPU_EXTRA_FIELDS.iter())
                    .map(|s| s.to_string())
                    .collect();
                sampler.extra = GPU_EXTRA_FIELDS.iter().map(|s| s.to_string()).collect();
                boot_log(&format!(
                    "[info] GpuSampler #{} initialized with native NVML",
                    gpu_index
                ));
                return sampler;
            }
        }
        sampler.probe();
        sampler
    }

    /// 初始化原生 NVML 驱动库（自动尝试默认路径与常见系统目录）
    fn init_nvml() -> Option<Nvml> {
        if let Ok(n) = Nvml::init() {
            return Some(n);
        }
        for path in [
            r"C:\Windows\System32\nvml.dll",
            r"C:\Program Files\NVIDIA Corporation\NVSMI\nvml.dll",
        ] {
            if Path::new(path).is_file() {
                if let Ok(n) = Nvml::builder().lib_path(OsStr::new(path)).init() {
                    return Some(n);
                }
            }
        }
        None
    }

    /// 列出本机所有 NVIDIA 显卡索引及型号（供设置界面选择）
    pub fn list_gpus() -> Vec<(usize, String)> {
        if let Some(nvml) = Self::init_nvml() {
            if let Ok(count) = nvml.device_count() {
                let mut list = Vec::new();
                for i in 0..count {
                    if let Ok(dev) = nvml.device_by_index(i) {
                        if let Ok(name) = dev.name() {
                            list.push((i as usize, name));
                        }
                    }
                }
                if !list.is_empty() {
                    return list;
                }
            }
        }
        let exe = Self::find_exe();
        let (rc, out) = run_hidden(
            &exe,
            &["--query-gpu=index,name", "--format=csv,noheader"],
            Duration::from_secs(5),
        );
        if rc != 0 {
            return Vec::new();
        }
        out.lines()
            .filter_map(|l| {
                let (idx_s, name) = l.split_once(',')?;
                let idx = idx_s.trim().parse::<usize>().ok()?;
                Some((idx, name.trim().to_string()))
            })
            .collect()
    }

    fn find_exe() -> String {
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path_var) {
                let candidate = dir.join("nvidia-smi.exe");
                if candidate.is_file() {
                    return candidate.to_string_lossy().into_owned();
                }
            }
        }
        let fallback = r"C:\Windows\System32\nvidia-smi.exe";
        if Path::new(fallback).is_file() {
            return fallback.to_string();
        }
        "nvidia-smi".to_string()
    }

    fn query(&self, fields: &[String], timeout: Duration) -> (i32, String) {
        let idx_s = self.gpu_index.to_string();
        let q = format!("--query-gpu={}", fields.join(","));
        run_hidden(
            &self.exe,
            &["-i", &idx_s, &q, "--format=csv,noheader,nounits"],
            timeout,
        )
    }

    fn probe(&mut self) {
        let all: Vec<String> = GPU_CORE_FIELDS
            .iter()
            .chain(GPU_EXTRA_FIELDS.iter())
            .map(|s| s.to_string())
            .collect();
        let (rc, out) = self.query(&all, Duration::from_secs(5));
        if rc == 0 && !out.trim().is_empty() {
            self.fields = all;
            self.extra = GPU_EXTRA_FIELDS.iter().map(|s| s.to_string()).collect();
            return;
        }
        let core: Vec<String> = GPU_CORE_FIELDS.iter().map(|s| s.to_string()).collect();
        let (rc2, out2) = self.query(&core, Duration::from_secs(5));
        self.fields = core;
        self.extra.clear();
        if rc2 != 0 {
            let snippet: String = out2.trim().chars().take(200).collect();
            boot_log(&format!(
                "[warn] nvidia-smi probe failed rc={} out={}",
                rc2, snippet
            ));
        }
    }

    pub fn static_query(&self) -> String {
        if let Some(ref nvml) = self.nvml {
            if let Ok(dev) = nvml.device_by_index(self.gpu_index as u32) {
                let name = dev.name().unwrap_or_else(|_| "NVIDIA GPU".to_string());
                let driver = nvml.sys_driver_version().unwrap_or_default();
                let mem_mb = dev
                    .memory_info()
                    .map(|m| format!("{} MiB", m.total / (1024 * 1024)))
                    .unwrap_or_default();
                let p_lim = dev
                    .power_management_limit()
                    .map(|mw| format!("{:.2} W", mw as f64 / 1000.0))
                    .unwrap_or_default();
                let p_max = dev
                    .power_management_limit_constraints()
                    .map(|c| format!("{:.2} W", c.max_limit as f64 / 1000.0))
                    .unwrap_or_default();
                let gen_max = dev
                    .max_pcie_link_gen()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let width_max = dev
                    .max_pcie_link_width()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let vbios = dev.vbios_version().unwrap_or_default();
                return format!(
                    "{}, {}, {}, {}, {}, {}, {}, {}",
                    name, driver, mem_mb, p_lim, p_max, gen_max, width_max, vbios
                );
            }
        }
        let idx_s = self.gpu_index.to_string();
        let (rc, out) = run_hidden(
            &self.exe,
            &[
                "-i",
                &idx_s,
                "--query-gpu=name,driver_version,memory.total,power.limit,power.max_limit,pcie.link.gen.max,pcie.link.width.max,vbios_version",
                "--format=csv,noheader",
            ],
            Duration::from_secs(10),
        );
        if rc == 0 {
            out.trim().to_string()
        } else {
            String::new()
        }
    }

    /// 解析基础显卡静态指纹：(型号, 驱动版本, 总显存MB, 最高PCIe规格, VBIOS)
    pub fn env_info(&self) -> (String, String, u64, String, String) {
        let raw = self.static_query();
        let parts: Vec<&str> = raw.split(',').map(|s| s.trim()).collect();
        if parts.len() >= 8 {
            let name = parts[0].to_string();
            let driver = parts[1].to_string();
            let vram_mb = parts[2]
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let pcie_max = if !parts[5].is_empty() && !parts[6].is_empty() {
                format!("Gen{}x{}", parts[5], parts[6])
            } else {
                "—".to_string()
            };
            let vbios = parts[7].to_string();
            return (name, driver, vram_mb, pcie_max, vbios);
        }
        ("NVIDIA GPU".to_string(), String::new(), 0, "—".to_string(), String::new())
    }

    /// 通过原生 NVML 直采单次显卡传感器数据
    fn sample_nvml(&self) -> Result<HashMap<String, String>, String> {
        let nvml = self
            .nvml
            .as_ref()
            .ok_or_else(|| "NVML 未初始化".to_string())?;
        let dev = nvml
            .device_by_index(self.gpu_index as u32)
            .map_err(|e| format!("NVML 获取设备失败: {}", e))?;

        let mut rec = HashMap::new();
        let temp = dev
            .temperature(TemperatureSensor::Gpu)
            .map_err(|e| format!("NVML 温度读取失败: {}", e))?;
        rec.insert("temperature.gpu".to_string(), temp.to_string());

        let power_draw = dev
            .power_usage()
            .map(|mw| format!("{:.2}", mw as f64 / 1000.0))
            .unwrap_or_else(|_| "0.00".to_string());
        rec.insert("power.draw".to_string(), power_draw);

        let power_limit = dev
            .enforced_power_limit()
            .or_else(|_| dev.power_management_limit())
            .map(|mw| format!("{:.2}", mw as f64 / 1000.0))
            .unwrap_or_else(|_| "0.00".to_string());
        rec.insert("power.limit".to_string(), power_limit);

        let clk_gfx = dev.clock_info(Clock::Graphics).unwrap_or(0);
        let clk_mem = dev.clock_info(Clock::Memory).unwrap_or(0);
        rec.insert("clocks.current.graphics".to_string(), clk_gfx.to_string());
        rec.insert("clocks.current.memory".to_string(), clk_mem.to_string());

        let pcie_gen = dev.current_pcie_link_gen().unwrap_or(0);
        let pcie_width = dev.current_pcie_link_width().unwrap_or(0);
        rec.insert("pcie.link.gen.current".to_string(), pcie_gen.to_string());
        rec.insert(
            "pcie.link.width.current".to_string(),
            pcie_width.to_string(),
        );

        if let Ok(gen_max) = dev.max_pcie_link_gen() {
            rec.insert("pcie.link.gen.max".to_string(), gen_max.to_string());
        }
        if let Ok(width_max) = dev.max_pcie_link_width() {
            rec.insert("pcie.link.width.max".to_string(), width_max.to_string());
        }

        if let Ok(util) = dev.utilization_rates() {
            rec.insert("utilization.gpu".to_string(), util.gpu.to_string());
            rec.insert("utilization.memory".to_string(), util.memory.to_string());
        } else {
            rec.insert("utilization.gpu".to_string(), "0".to_string());
            rec.insert("utilization.memory".to_string(), "0".to_string());
        }

        if let Ok(mem) = dev.memory_info() {
            rec.insert(
                "memory.used".to_string(),
                (mem.used / (1024 * 1024)).to_string(),
            );
            rec.insert(
                "memory.total".to_string(),
                (mem.total / (1024 * 1024)).to_string(),
            );
        }

        let fan = dev
            .fan_speed(0)
            .map(|f| f.to_string())
            .unwrap_or_else(|_| "[N/A]".to_string());
        rec.insert("fan.speed".to_string(), fan);

        let thr = dev
            .current_throttle_reasons()
            .map(|r| format!("0x{:016x}", r.bits()))
            .unwrap_or_else(|_| "0x0000000000000000".to_string());
        rec.insert("clocks_throttle_reasons.active".to_string(), thr);

        if let Some(ref hs) = self.nvapi_hotspot {
            if let Some(hs_temp) = hs.read_hotspot(Some(temp as f64)) {
                rec.insert("temperature.hotspot".to_string(), format!("{:.2}", hs_temp));
            }
        }

        Ok(rec)
    }

    /// 采集单次显卡样本，优先原生 NVML 直采，失败自动回退 nvidia-smi.exe，返回 (字段映射或错误原因, 耗时毫秒)
    pub fn sample(&self) -> (Result<HashMap<String, String>, String>, u128) {
        let t0 = Instant::now();
        if self.nvml.is_some() {
            if let Ok(rec) = self.sample_nvml() {
                return (Ok(rec), t0.elapsed().as_millis());
            }
        }
        let (rc, out) = self.query(&self.fields, Duration::from_secs(4));
        let ms = t0.elapsed().as_millis();
        if rc != 0 || out.trim().is_empty() {
            let err: String = out.trim().chars().take(160).collect();
            return (Err(err), ms);
        }
        let first_line = out.trim().lines().next().unwrap_or("");
        let mut rec = HashMap::new();
        for (k, v) in self.fields.iter().zip(first_line.split(',')) {
            rec.insert(k.clone(), v.trim().to_string());
        }
        if let Some(ref hs) = self.nvapi_hotspot {
            let core = rec
                .get("temperature.gpu")
                .and_then(|s| s.trim().parse::<f64>().ok());
            if let Some(hs_temp) = hs.read_hotspot(core) {
                rec.insert("temperature.hotspot".to_string(), format!("{:.2}", hs_temp));
            }
        }
        (Ok(rec), ms)
    }
}

/// 系统内存样本
pub struct MemSample {
    pub commit_gb: f64,
    pub commit_limit_gb: f64,
    pub phys_avail_mb: u64,
    pub phys_total_mb: u64,
    pub sys_cache_mb: u64,
    pub kernel_paged_mb: u64,
    pub kernel_nonpaged_mb: u64,
}

/// 内存采集器（调用原生 K32GetPerformanceInfo）
pub struct MemSampler;

impl MemSampler {
    pub fn sample(&self) -> Option<MemSample> {
        let mut pi: PERFORMANCE_INFORMATION = unsafe { std::mem::zeroed() };
        pi.cb = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
        let ok = unsafe { K32GetPerformanceInfo(&mut pi, pi.cb) };
        if ok == 0 {
            return None;
        }
        let ps = if pi.PageSize == 0 {
            4096u64
        } else {
            pi.PageSize as u64
        };
        let gb = (1024u64 * 1024 * 1024) as f64;
        let mb = 1024u64 * 1024;
        let round2 = |v: f64| (v * 100.0).round() / 100.0;
        Some(MemSample {
            commit_gb: round2((pi.CommitTotal as u64 * ps) as f64 / gb),
            commit_limit_gb: round2((pi.CommitLimit as u64 * ps) as f64 / gb),
            phys_avail_mb: (pi.PhysicalAvailable as u64 * ps) / mb,
            phys_total_mb: (pi.PhysicalTotal as u64 * ps) / mb,
            sys_cache_mb: (pi.SystemCache as u64 * ps) / mb,
            kernel_paged_mb: (pi.KernelPaged as u64 * ps) / mb,
            kernel_nonpaged_mb: (pi.KernelNonpaged as u64 * ps) / mb,
        })
    }
}

/// 进程信息
pub struct ProcInfo {
    pub name: String,
    pub pid: String,
    pub ws_mb: f64,
    pub cpu_s: String,
}

/// 目标游戏进程采集器（使用原生 Toolhelp32 快照，零子进程创建）
pub struct ProcSampler {
    names: HashSet<String>,
}

impl ProcSampler {
    pub fn new(names: &[String]) -> Self {
        let mut set = HashSet::new();
        for n in names {
            let low = n.to_lowercase();
            if !low.ends_with(".exe") {
                set.insert(format!("{}.exe", low));
            }
            set.insert(low);
        }
        Self { names: set }
    }

    pub fn sample(&mut self) -> Vec<ProcInfo> {
        let mut found = Vec::new();
        if self.names.is_empty() {
            return found;
        }
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return found;
            }
            let mut pe: PROCESSENTRY32W = std::mem::zeroed();
            pe.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(snap, &mut pe) != 0 {
                loop {
                    let len = pe
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(pe.szExeFile.len());
                    let exe_name = String::from_utf16_lossy(&pe.szExeFile[..len]);
                    if self.names.contains(&exe_name.to_lowercase()) {
                        let (ws_mb, cpu_s) = Self::query_proc_metrics(pe.th32ProcessID);
                        found.push(ProcInfo {
                            name: exe_name,
                            pid: pe.th32ProcessID.to_string(),
                            ws_mb,
                            cpu_s,
                        });
                    }
                    if Process32NextW(snap, &mut pe) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);
        }
        found
    }

    unsafe fn query_proc_metrics(pid: u32) -> (f64, String) {
        let mut h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid);
        if h.is_null() {
            h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        }
        if h.is_null() {
            return (0.0, String::new());
        }
        let mut pmc: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let ws_mb = if K32GetProcessMemoryInfo(h, &mut pmc, pmc.cb) != 0 {
            ((pmc.WorkingSetSize as f64 / 1024.0 / 1024.0) * 10.0).round() / 10.0
        } else {
            0.0
        };

        let mut ct: FILETIME = std::mem::zeroed();
        let mut et: FILETIME = std::mem::zeroed();
        let mut kt: FILETIME = std::mem::zeroed();
        let mut ut: FILETIME = std::mem::zeroed();
        let cpu_s = if GetProcessTimes(h, &mut ct, &mut et, &mut kt, &mut ut) != 0 {
            let k = ((kt.dwHighDateTime as u64) << 32) | (kt.dwLowDateTime as u64);
            let u = ((ut.dwHighDateTime as u64) << 32) | (ut.dwLowDateTime as u64);
            let secs = (k + u) as f64 / 10_000_000.0;
            format!("{:.1}", secs)
        } else {
            String::new()
        };
        CloseHandle(h);
        (ws_mb, cpu_s)
    }
}

/// 原生 Windows Event Log 采集器（替代常驻 powershell.exe，节省 70MB+ 内存）
pub struct EventTailer {
    first: bool,
    last_sys: u64,
    last_app: u64,
    last_poll: Instant,
    xml_buf: Vec<u16>,
    msg_buf: Vec<u16>,
}

impl EventTailer {
    pub fn new() -> Self {
        Self {
            first: true,
            last_sys: 0,
            last_app: 0,
            last_poll: Instant::now() - Duration::from_secs(5),
            xml_buf: vec![0u16; 16384],
            msg_buf: vec![0u16; 4096],
        }
    }

    pub fn poll(&mut self, _timeout: Duration) -> Vec<Value> {
        // 控制轮询间隔至少 1.5 秒，避免无谓消耗
        if !self.first && self.last_poll.elapsed() < Duration::from_millis(1500) {
            return Vec::new();
        }
        self.last_poll = Instant::now();
        let mut out = Vec::new();

        if self.first {
            self.first = false;
            // 首次回填最近 15 条 System 事件用于保留崩溃上下文
            let mut sys_evs = self.query_channel("System", "*", 15, true);
            for ev in &sys_evs {
                if let Some(rid) = ev.get("rid").and_then(|v| v.as_u64()) {
                    self.last_sys = self.last_sys.max(rid);
                }
            }
            sys_evs.reverse();
            out.extend(sys_evs);

            // 记录 Application 当前最大 RecordId
            let app_latest = self.query_channel("Application", "*", 1, true);
            if let Some(rid) = app_latest
                .first()
                .and_then(|v| v.get("rid"))
                .and_then(|v| v.as_u64())
            {
                self.last_app = rid;
            }
            boot_log("[info] native event tailer started");
            return out;
        }

        // 增量查询 System
        let sys_q = format!("*[System[EventRecordID > {}]]", self.last_sys);
        let sys_new = self.query_channel("System", &sys_q, 200, false);
        for ev in sys_new {
            if let Some(rid) = ev.get("rid").and_then(|v| v.as_u64()) {
                self.last_sys = self.last_sys.max(rid);
            }
            out.push(ev);
        }

        // 增量查询 Application（关注崩溃/错误/关键级别事件）
        let app_q = format!(
            "*[System[EventRecordID > {} and ((Level=1 or Level=2) or EventID=1000 or EventID=1001 or EventID=1002 or EventID=1003 or EventID=1026)]]",
            self.last_app
        );
        let app_new = self.query_channel("Application", &app_q, 100, false);
        for ev in app_new {
            if let Some(rid) = ev.get("rid").and_then(|v| v.as_u64()) {
                self.last_app = self.last_app.max(rid);
            }
            out.push(ev);
        }

        out
    }

    fn query_channel(
        &mut self,
        channel: &str,
        xpath: &str,
        max_count: usize,
        reverse: bool,
    ) -> Vec<Value> {
        let mut results = Vec::new();
        let ch_w = to_wide(channel);
        let q_w = to_wide(xpath);
        let flags = EvtQueryChannelPath | if reverse { EvtQueryReverseDirection } else { 0 };

        unsafe {
            let h_query = EvtQuery(0, ch_w.as_ptr(), q_w.as_ptr(), flags);
            if h_query == 0 {
                return results;
            }

            let mut handles: [EVT_HANDLE; 32] = [0; 32];
            while results.len() < max_count {
                let want = (max_count - results.len()).min(handles.len()) as u32;
                let mut returned: u32 = 0;
                let ok = EvtNext(h_query, want, handles.as_mut_ptr(), 0, 0, &mut returned);
                if ok == 0 || returned == 0 {
                    break;
                }
                for i in 0..(returned as usize) {
                    let ev_h = handles[i];
                    if let Some(val) = self.decode_event(channel, ev_h) {
                        results.push(val);
                    }
                    EvtClose(ev_h);
                }
            }
            EvtClose(h_query);
        }
        results
    }

    unsafe fn decode_event(&mut self, channel: &str, ev_h: EVT_HANDLE) -> Option<Value> {
        let mut used: u32 = 0;
        let mut prop_cnt: u32 = 0;
        let ok = EvtRender(
            0,
            ev_h,
            EvtRenderEventXml,
            (self.xml_buf.len() * 2) as u32,
            self.xml_buf.as_mut_ptr() as *mut _,
            &mut used,
            &mut prop_cnt,
        );
        if ok == 0 {
            let needed = (used as usize / 2) + 1;
            if needed > self.xml_buf.len() && needed <= 262144 {
                self.xml_buf.resize(needed, 0);
                let ok2 = EvtRender(
                    0,
                    ev_h,
                    EvtRenderEventXml,
                    (self.xml_buf.len() * 2) as u32,
                    self.xml_buf.as_mut_ptr() as *mut _,
                    &mut used,
                    &mut prop_cnt,
                );
                if ok2 == 0 {
                    return None;
                }
            } else {
                return None;
            }
        }

        let char_len = (used as usize / 2).min(self.xml_buf.len());
        let xml = String::from_utf16_lossy(&self.xml_buf[..char_len])
            .trim_end_matches('\0')
            .to_string();

        let prov = extract_attr(&xml, "<Provider", "Name=").unwrap_or_default();
        let eid = extract_tag_text(&xml, "EventID")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(-1);
        let level_num = extract_tag_text(&xml, "Level")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(4);
        let level = match level_num {
            1 => "Critical",
            2 => "Error",
            3 => "Warning",
            4 | 0 => "Information",
            5 => "Verbose",
            _ => "Information",
        };
        let ts = extract_attr(&xml, "<TimeCreated", "SystemTime=").unwrap_or_else(now_iso);
        let rid = extract_tag_text(&xml, "EventRecordID")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let mut msg = self.format_event_message(&prov, ev_h).unwrap_or_default();
        if msg.is_empty() {
            msg = extract_eventdata_fallback(&xml);
        }
        let first_line = msg
            .lines()
            .map(|l| l.trim())
            .find(|l| !l.is_empty())
            .unwrap_or("")
            .to_string();

        Some(json!({
            "log": channel,
            "ts": ts,
            "id": eid,
            "level": level,
            "prov": prov,
            "rid": rid,
            "msg": first_line,
        }))
    }

    unsafe fn format_event_message(&mut self, prov: &str, ev_h: EVT_HANDLE) -> Option<String> {
        if prov.is_empty() {
            return None;
        }
        let prov_w = to_wide(prov);
        let h_meta = EvtOpenPublisherMetadata(0, prov_w.as_ptr(), std::ptr::null(), 0, 0);
        if h_meta == 0 {
            return None;
        }
        let mut used: u32 = 0;
        let ok = EvtFormatMessage(
            h_meta,
            ev_h,
            0,
            0,
            std::ptr::null(),
            EvtFormatMessageEvent,
            self.msg_buf.len() as u32,
            self.msg_buf.as_mut_ptr(),
            &mut used,
        );
        let res = if ok != 0 {
            let len = (used as usize).min(self.msg_buf.len());
            Some(
                String::from_utf16_lossy(&self.msg_buf[..len])
                    .trim_end_matches('\0')
                    .to_string(),
            )
        } else {
            None
        };
        EvtClose(h_meta);
        res
    }

    pub fn close(&mut self) {}
}

fn extract_attr(xml: &str, tag_prefix: &str, attr_eq: &str) -> Option<String> {
    let start = xml.find(tag_prefix)?;
    let slice = &xml[start..];
    let tag_end = slice.find('>')?;
    let tag_slice = &slice[..tag_end];
    let a_pos = tag_slice.find(attr_eq)?;
    let after = &tag_slice[a_pos + attr_eq.len()..];
    let quote = after.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let rest = &after[1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn extract_tag_text(xml: &str, tag: &str) -> Option<String> {
    let open_prefix = format!("<{}", tag);
    let pos = xml.find(&open_prefix)?;
    let after = &xml[pos + open_prefix.len()..];
    let gt = after.find('>')?;
    let body = &after[gt + 1..];
    let close_tag = format!("</{}>", tag);
    let end = body.find(&close_tag)?;
    Some(body[..end].trim().to_string())
}

fn extract_eventdata_fallback(xml: &str) -> String {
    let mut parts = Vec::new();
    let mut rest = xml;
    while let Some(pos) = rest.find("<Data") {
        let after = &rest[pos + 5..];
        let Some(gt) = after.find('>') else { break };
        let body = &after[gt + 1..];
        let Some(end) = body.find("</Data>") else {
            break;
        };
        let val = body[..end].trim();
        if !val.is_empty() {
            parts.push(val.to_string());
        }
        rest = &body[end + 7..];
        if parts.len() >= 6 {
            break;
        }
    }
    parts.join(" | ")
}

/// 崩溃报告监视条目
#[derive(Debug, Clone, serde::Serialize)]
pub struct WerItem {
    pub kind: String,
    pub name: String,
    pub path: String,
    pub created: String,
}

/// WER 队列/归档与 LiveKernelReports 监视器
pub struct WerWatcher {
    pub bases: Vec<PathBuf>,
    pub live_dir: PathBuf,
    seen: HashSet<PathBuf>,
}

impl WerWatcher {
    pub fn new() -> Self {
        let pd = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".to_string());
        let bases = vec![
            PathBuf::from(&pd).join(r"Microsoft\Windows\WER\ReportQueue"),
            PathBuf::from(&pd).join(r"Microsoft\Windows\WER\ReportArchive"),
        ];
        let live_dir = PathBuf::from(r"C:\Windows\LiveKernelReports");
        let mut w = Self {
            bases,
            live_dir,
            seen: HashSet::new(),
        };
        w.baseline();
        w
    }

    fn list_subdirs(base: &Path) -> Vec<(String, PathBuf)> {
        let mut res = Vec::new();
        if let Ok(entries) = fs::read_dir(base) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    res.push((name, p));
                }
            }
        }
        res
    }

    fn walk_files(root: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(root) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    Self::walk_files(&p, out);
                } else if p.is_file() {
                    out.push(p);
                }
            }
        }
    }

    fn baseline(&mut self) {
        for b in &self.bases {
            for (_, p) in Self::list_subdirs(b) {
                self.seen.insert(p);
            }
        }
        let mut files = Vec::new();
        Self::walk_files(&self.live_dir, &mut files);
        for f in files {
            self.seen.insert(f);
        }
    }

    fn format_ctime(p: &Path) -> String {
        fs::metadata(p)
            .and_then(|m| m.created())
            .map(|st: SystemTime| {
                let dt: DateTime<Local> = st.into();
                dt.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
            })
            .unwrap_or_default()
    }

    pub fn poll(&mut self) -> Vec<WerItem> {
        let mut new_items = Vec::new();
        for b in &self.bases {
            for (name, path) in Self::list_subdirs(b) {
                if self.seen.contains(&path) {
                    continue;
                }
                self.seen.insert(path.clone());
                let kind = if name.to_lowercase().starts_with("kernel_") {
                    "KERNEL_REPORT"
                } else {
                    "WER"
                };
                new_items.push(WerItem {
                    kind: kind.to_string(),
                    name,
                    created: Self::format_ctime(&path),
                    path: path.to_string_lossy().into_owned(),
                });
            }
        }
        let mut files = Vec::new();
        Self::walk_files(&self.live_dir, &mut files);
        for p in files {
            if self.seen.contains(&p) {
                continue;
            }
            self.seen.insert(p.clone());
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            new_items.push(WerItem {
                kind: "LIVEDUMP".to_string(),
                name,
                created: now_iso(),
                path: p.to_string_lossy().into_owned(),
            });
        }
        new_items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "需要本机 NVIDIA 显卡与驱动环境"]
    fn test_nvapi_diag() {
        let sampler = GpuSampler::new(0);
        let (res, ms) = sampler.sample();
        let rec = res.expect("GPU sample failed");
        println!(
            "ms={} temp={:?} hotspot={:?}",
            ms,
            rec.get("temperature.gpu"),
            rec.get("temperature.hotspot")
        );
        assert!(
            rec.get("temperature.hotspot").is_some(),
            "hotspot not collected"
        );
    }
}
