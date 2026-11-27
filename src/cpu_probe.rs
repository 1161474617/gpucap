//! 实时 CPU 状态探针（仅用于内存态实时监测）：
//! 结合 sysinfo 0.40、wmi 0.18 与 Windows GetLogicalProcessorInformationEx
//! 获取 CPU 型号、温度、总使用率、P/E 大小核汇总及逐逻辑核心使用率与频率。

use std::ptr::null_mut;

use serde::{Deserialize, Serialize};
use sysinfo::{CpuRefreshKind, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
use windows_sys::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, RelationProcessorCore,
    SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};
use wmi::WMIConnection;

#[derive(Clone, Serialize, Default)]
pub struct CpuCoreInfo {
    pub id: usize,
    pub kind: &'static str,
    pub usage: f32,
    pub freq_mhz: u64,
}

#[derive(Clone, Serialize, Default)]
pub struct TopProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f32,
    pub mem_mb: u64,
}

#[derive(Clone, Serialize, Default)]
pub struct CpuSnapshot {
    pub brand: String,
    pub temp_c: Option<f64>,
    pub total_usage: f32,
    pub avg_freq_mhz: u64,
    pub max_freq_mhz: u64,
    pub physical_cores: usize,
    pub logical_cores: usize,
    pub has_hybrid: bool,
    pub p_count: usize,
    pub p_usage: f32,
    pub p_freq_mhz: u64,
    pub e_count: usize,
    pub e_usage: f32,
    pub e_freq_mhz: u64,
    pub cores: Vec<CpuCoreInfo>,
    pub top_cpu_procs: Vec<TopProcessInfo>,
    pub top_mem_procs: Vec<TopProcessInfo>,
}

#[derive(Deserialize)]
#[serde(rename = "MSAcpi_ThermalZoneTemperature")]
#[serde(rename_all = "PascalCase")]
struct AcpiThermalZone {
    current_temperature: u32,
}

#[derive(Deserialize)]
#[serde(rename = "Win32_PerfFormattedData_Counters_ThermalZoneInformation")]
#[serde(rename_all = "PascalCase")]
struct PerfThermalZone {
    temperature: u32,
}

pub struct CpuProbe {
    sys: System,
    core_kinds: Vec<&'static str>,
    has_hybrid: bool,
    physical_cores: usize,
    wmi_root_wmi: Option<WMIConnection>,
    wmi_cimv2: Option<WMIConnection>,
    brand: String,
}

impl CpuProbe {
    pub fn new() -> Self {
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_processes(ProcessRefreshKind::nothing().with_cpu().with_memory()),
        )
        .or_else(|_| System::new())
        .expect("初始化 CPU 系统探针失败");
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        sys.refresh_cpu_all();
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );

        let logical = sys.cpus().len();
        let brand = sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "CPU".to_string());
        let physical_cores = System::physical_core_count().unwrap_or(logical.max(1));

        let (core_kinds, has_hybrid) = detect_core_topology(logical);

        let wmi_root_wmi = WMIConnection::with_namespace_path(r"ROOT\WMI").ok();
        let wmi_cimv2 = WMIConnection::new().ok();

        Self {
            sys,
            core_kinds,
            has_hybrid,
            physical_cores,
            wmi_root_wmi,
            wmi_cimv2,
            brand,
        }
    }

    pub fn sample(&mut self) -> CpuSnapshot {
        self.sys.refresh_cpu_all();
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let cpus = self.sys.cpus();
        let logical = cpus.len();
        let total_usage = (self.sys.global_cpu_usage() * 10.0).round() / 10.0;

        let mut cores = Vec::with_capacity(logical);
        let mut sum_freq = 0u64;
        let mut max_freq = 0u64;

        let mut p_count = 0usize;
        let mut p_usage_sum = 0.0f32;
        let mut p_freq_sum = 0u64;

        let mut e_count = 0usize;
        let mut e_usage_sum = 0.0f32;
        let mut e_freq_sum = 0u64;

        for (i, cpu) in cpus.iter().enumerate() {
            let kind = self.core_kinds.get(i).copied().unwrap_or("Std");
            let usage = (cpu.usage() * 10.0).round() / 10.0;
            let freq = cpu.frequency();
            sum_freq += freq;
            if freq > max_freq {
                max_freq = freq;
            }
            if kind == "P" {
                p_count += 1;
                p_usage_sum += usage;
                p_freq_sum += freq;
            } else if kind == "E" {
                e_count += 1;
                e_usage_sum += usage;
                e_freq_sum += freq;
            }
            cores.push(CpuCoreInfo {
                id: i,
                kind,
                usage,
                freq_mhz: freq,
            });
        }

        let avg_freq_mhz = if logical > 0 {
            sum_freq / logical as u64
        } else {
            0
        };
        let round1 = |v: f32| (v * 10.0).round() / 10.0;
        let logical_f = logical.max(1) as f32;
        let mut procs: Vec<TopProcessInfo> = self
            .sys
            .processes()
            .iter()
            .filter_map(|(pid, proc)| {
                let raw_pid = pid.as_u32();
                if raw_pid == 0 {
                    return None;
                }
                let name = proc.name().to_string_lossy().into_owned();
                if name.eq_ignore_ascii_case("System Idle Process") {
                    return None;
                }
                let cpu_pct = round1((proc.cpu_usage() / logical_f).clamp(0.0, 100.0));
                let mem_mb = proc.memory() / (1024 * 1024);
                Some(TopProcessInfo {
                    pid: raw_pid,
                    name,
                    cpu_pct,
                    mem_mb,
                })
            })
            .collect();
        let (top_cpu_procs, top_mem_procs) = pick_top_processes(&mut procs, 5);

        CpuSnapshot {
            brand: self.brand.clone(),
            temp_c: self.read_temp_c(),
            total_usage,
            avg_freq_mhz,
            max_freq_mhz: max_freq,
            physical_cores: self.physical_cores,
            logical_cores: logical,
            has_hybrid: self.has_hybrid,
            p_count,
            p_usage: if p_count > 0 {
                round1(p_usage_sum / p_count as f32)
            } else {
                0.0
            },
            p_freq_mhz: if p_count > 0 {
                p_freq_sum / p_count as u64
            } else {
                0
            },
            e_count,
            e_usage: if e_count > 0 {
                round1(e_usage_sum / e_count as f32)
            } else {
                0.0
            },
            e_freq_mhz: if e_count > 0 {
                e_freq_sum / e_count as u64
            } else {
                0
            },
            cores,
            top_cpu_procs,
            top_mem_procs,
        }
    }

    fn read_temp_c(&self) -> Option<f64> {
        if let Some(ref conn) = self.wmi_root_wmi {
            if let Ok(zones) = conn.query::<AcpiThermalZone>() {
                let best = zones
                    .into_iter()
                    .map(|z| (z.current_temperature as f64) / 10.0 - 273.15)
                    .filter(|t| *t > 5.0 && *t < 125.0)
                    .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |a| a.max(v))));
                if best.is_some() {
                    return best.map(|v| (v * 10.0).round() / 10.0);
                }
            }
        }
        if let Some(ref conn) = self.wmi_cimv2 {
            if let Ok(zones) = conn.query::<PerfThermalZone>() {
                let best = zones
                    .into_iter()
                    .map(|z| (z.temperature as f64) - 273.15)
                    .filter(|t| *t > 5.0 && *t < 125.0)
                    .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |a| a.max(v))));
                if best.is_some() {
                    return best.map(|v| (v * 10.0).round() / 10.0);
                }
            }
        }
        None
    }
}

/// 调用 Windows API GetLogicalProcessorInformationEx(RelationProcessorCore)
/// 解析混合架构（P-Core / E-Core）EfficiencyClass 与每个逻辑核心的归属类型。
fn detect_core_topology(logical_count: usize) -> (Vec<&'static str>, bool) {
    let mut kinds = vec!["Std"; logical_count];
    if logical_count == 0 {
        return (kinds, false);
    }

    let mut len = 0u32;
    unsafe {
        GetLogicalProcessorInformationEx(RelationProcessorCore, null_mut(), &mut len);
    }
    if len == 0 {
        return (kinds, false);
    }

    let mut buf = vec![0u8; len as usize];
    let ok = unsafe {
        let rc = GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            buf.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
            &mut len,
        );
        rc != 0 || std::io::Error::last_os_error().raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
    };
    if !ok {
        return (kinds, false);
    }

    let mut core_effs: Vec<(u8, Vec<usize>)> = Vec::new();
    let mut offset = 0usize;
    while offset + std::mem::size_of::<u32>() * 2 <= len as usize {
        let ptr = unsafe { buf.as_ptr().add(offset) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX };
        let info = unsafe { &*ptr };
        let size = info.Size as usize;
        if size == 0 || offset + size > len as usize {
            break;
        }
        if info.Relationship == RelationProcessorCore {
            let proc = unsafe { &info.Anonymous.Processor };
            let eff = proc.EfficiencyClass;
            let gcount = proc.GroupCount as usize;
            let mut ids = Vec::new();
            for g in 0..gcount {
                let ga = unsafe { &*proc.GroupMask.as_ptr().add(g) };
                let base = (ga.Group as usize) * 64;
                let mask = ga.Mask;
                for bit in 0..usize::BITS as usize {
                    if (mask & (1usize << bit)) != 0 {
                        let idx = base + bit;
                        if idx < logical_count {
                            ids.push(idx);
                        }
                    }
                }
            }
            if !ids.is_empty() {
                core_effs.push((eff, ids));
            }
        }
        offset += size;
    }

    if core_effs.is_empty() {
        return (kinds, false);
    }

    let min_eff = core_effs.iter().map(|(e, _)| *e).min().unwrap_or(0);
    let max_eff = core_effs.iter().map(|(e, _)| *e).max().unwrap_or(0);
    if max_eff <= min_eff {
        return (kinds, false);
    }

    for (eff, ids) in core_effs {
        let label = if eff == max_eff { "P" } else { "E" };
        for id in ids {
            if id < kinds.len() {
                kinds[id] = label;
            }
        }
    }
    (kinds, true)
}

/// 从进程列表中分别按 CPU 占用和内存占用提取前 N 个进程。
pub fn pick_top_processes(
    procs: &mut [TopProcessInfo],
    limit: usize,
) -> (Vec<TopProcessInfo>, Vec<TopProcessInfo>) {
    procs.sort_by(|a, b| {
        b.cpu_pct
            .partial_cmp(&a.cpu_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.mem_mb.cmp(&a.mem_mb))
    });
    let top_cpu = procs.iter().take(limit).cloned().collect();

    procs.sort_by(|a, b| {
        b.mem_mb.cmp(&a.mem_mb).then_with(|| {
            b.cpu_pct
                .partial_cmp(&a.cpu_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    let top_mem = procs.iter().take(limit).cloned().collect();
    (top_cpu, top_mem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pick_top_processes_limits_and_sorts() {
        let mut list = vec![
            TopProcessInfo { pid: 1, name: "a".into(), cpu_pct: 5.0, mem_mb: 500 },
            TopProcessInfo { pid: 2, name: "b".into(), cpu_pct: 42.5, mem_mb: 120 },
            TopProcessInfo { pid: 3, name: "c".into(), cpu_pct: 12.0, mem_mb: 2400 },
            TopProcessInfo { pid: 4, name: "d".into(), cpu_pct: 1.0, mem_mb: 80 },
            TopProcessInfo { pid: 5, name: "e".into(), cpu_pct: 25.0, mem_mb: 900 },
            TopProcessInfo { pid: 6, name: "f".into(), cpu_pct: 8.0, mem_mb: 300 },
        ];
        let (by_cpu, by_mem) = pick_top_processes(&mut list, 5);
        assert_eq!(by_cpu.len(), 5);
        assert_eq!(by_cpu[0].pid, 2);
        assert_eq!(by_cpu[1].pid, 5);
        assert_eq!(by_mem.len(), 5);
        assert_eq!(by_mem[0].pid, 3);
        assert_eq!(by_mem[1].pid, 5);
    }
}
