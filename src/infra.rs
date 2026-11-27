//! 基础设施模块：路径解析、时间格式化、配置加载、轮转日志、进程与文件锁、无窗口命令执行。

use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HANDLE, STILL_ACTIVE,
};
use windows_sys::Win32::Storage::FileSystem::{
    LockFileEx, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
};
use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, REG_BINARY,
    REG_SZ,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, GetExitCodeProcess, OpenEventW, OpenProcess, SetEvent,
    WaitForSingleObject, EVENT_MODIFY_STATE, INFINITE, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, SetForegroundWindow, ShowWindow,
    SW_HIDE, SW_RESTORE, SW_SHOWNORMAL,
};
use windows_sys::Win32::System::IO::OVERLAPPED;

pub const APP_NAME: &str = "GPUCap";
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub const DETACHED_PROCESS: u32 = 0x0000_0008;
pub const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

/// 计算程序基准目录：
/// 若当前可执行文件位于 cargo 的 target 构建目录下且当前工作目录存在 Cargo.toml，
/// 则使用当前工作目录，方便开发调试；否则使用可执行文件所在目录。
pub fn base_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let parent_str = parent.to_string_lossy().to_lowercase();
            if (parent_str.ends_with("\\target\\debug") || parent_str.ends_with("\\target\\release"))
                && Path::new("Cargo.toml").exists()
            {
                if let Ok(cwd) = std::env::current_dir() {
                    return cwd;
                }
            }
            return parent.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn data_dir() -> PathBuf {
    base_dir().join("data")
}

pub fn sessions_dir() -> PathBuf {
    data_dir().join("sessions")
}

pub fn incidents_dir() -> PathBuf {
    data_dir().join("incidents")
}

pub fn latest_file() -> PathBuf {
    data_dir().join("latest.txt")
}

pub fn lock_file() -> PathBuf {
    data_dir().join("gpucap.lock")
}

pub fn pid_file() -> PathBuf {
    data_dir().join("gpucap.pid")
}

pub fn stop_file() -> PathBuf {
    data_dir().join("STOP")
}

pub fn config_file() -> PathBuf {
    data_dir().join("config.json")
}

/// 返回本地时区精确到毫秒的 ISO 8601 时间字符串，与原版 Python 格式一致
pub fn now_iso() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string()
}

/// 返回本地时区精确到秒的 ISO 8601 时间字符串
pub fn now_iso_sec() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// 写入启动/守护日志，永不因写日志失败而中断采集
pub fn boot_log(msg: &str) {
    let line = format!("{} {}", now_iso(), msg);
    println!("{}", line);
    let dir = data_dir();
    let _ = fs::create_dir_all(&dir);
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("daemon.boot.log"))
    {
        let _ = writeln!(f, "{}", line);
    }
}

/// 采集配置，字段与原版 config.json 完全兼容
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub gpu_interval: f64,
    pub mem_interval: f64,
    pub proc_interval: f64,
    pub wer_interval: f64,
    pub game_names: Vec<String>,
    pub rotate_mb: u64,
    pub max_session_hours: u64,
    pub temp_warn: f64,
    pub hotspot_warn: f64,
    pub hotspot_delta_warn: f64,
    pub power_warn: f64,
    pub cpu_temp_warn: f64,
    pub cpu_usage_warn: f64,
    pub vram_warn_pct: f64,
    pub pcie_drop_warn: bool,
    pub gpu_index: usize,
    pub minimize_to_tray: bool,
    pub web_port: u16,
    pub auth_pin: String,
    pub read_only_remote: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            gpu_interval: 1.0,
            mem_interval: 5.0,
            proc_interval: 5.0,
            wer_interval: 5.0,
            game_names: vec![
                "Cyberpunk2077.exe".to_string(),
                "Cyberpunk2077".to_string(),
            ],
            rotate_mb: 64,
            max_session_hours: 24,
            temp_warn: 78.0,
            hotspot_warn: 92.0,
            hotspot_delta_warn: 20.0,
            power_warn: 360.0,
            cpu_temp_warn: 90.0,
            cpu_usage_warn: 95.0,
            vram_warn_pct: 92.0,
            pcie_drop_warn: true,
            gpu_index: 0,
            minimize_to_tray: true,
            web_port: 8787,
            auth_pin: String::new(),
            read_only_remote: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvFingerprint {
    pub gpu_name: String,
    pub driver_version: String,
    pub vram_total_mb: u64,
    pub pcie_max: String,
    pub vbios: String,
    pub os_version: String,
    pub power_plan: String,
    pub driver_changed_from: Option<String>,
}

/// 读取当前 Windows 电源计划名称（隐藏窗口执行，失败时返回 "—"）。
pub fn detect_power_plan() -> String {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        if let Ok(out) = std::process::Command::new("powercfg")
            .arg("/getactivescheme")
            .creation_flags(0x08000000)
            .output()
        {
            if out.status.success() {
                let txt = String::from_utf8_lossy(&out.stdout);
                if let (Some(l), Some(r)) = (txt.rfind('('), txt.rfind(')')) {
                    if r > l + 1 {
                        return txt[l + 1..r].trim().to_string();
                    }
                }
            }
        }
    }
    "—".to_string()
}

/// 比对并更新上次记录的显卡驱动版本号；若发生变更则返回旧版本号。
pub fn check_and_update_driver_version(current: &str) -> Option<String> {
    let cur = current.trim();
    if cur.is_empty() {
        return None;
    }
    let dir = data_dir();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("last_driver.txt");
    let prev = fs::read_to_string(&path).ok().map(|s| s.trim().to_string());
    let changed = match prev {
        Some(ref old) if !old.is_empty() && old != cur => Some(old.clone()),
        _ => None,
    };
    let _ = fs::write(&path, cur);
    changed
}

/// 将 NVML `clocks_throttle_reasons.active` 十六进制/十进制掩码解码为中文短标签。
pub fn decode_throttle_label(raw_or_hex: &str) -> String {
    let trimmed = raw_or_hex.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let val = if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        u64::from_str_radix(trimmed, 16)
            .ok()
            .or_else(|| trimmed.parse::<u64>().ok())
    };
    let Some(mask) = val else {
        return trimmed.chars().take(10).collect();
    };
    if mask == 0 {
        return String::new();
    }
    let mut tags: Vec<&'static str> = Vec::new();
    if mask & 0x004 != 0 {
        tags.push("功耗墙");
    }
    if mask & 0x008 != 0 {
        tags.push("温度墙");
    }
    if mask & 0x020 != 0 {
        tags.push("软件温控");
    }
    if mask & 0x080 != 0 {
        tags.push("硬件降频");
    }
    if mask & 0x040 != 0 {
        tags.push("同步限频");
    }
    if mask & 0x100 != 0 {
        tags.push("显示限制");
    }
    if mask & 0x002 != 0 && tags.is_empty() {
        tags.push("应用限频");
    }
    if mask & 0x001 != 0 && tags.is_empty() {
        return "空闲".to_string();
    }
    if tags.is_empty() {
        format!("0x{:x}", mask)
    } else {
        tags.join("+")
    }
}

/// 加载配置，若不存在则自动写入默认配置
pub fn load_config() -> Config {
    let dir = data_dir();
    let _ = fs::create_dir_all(&dir);
    let path = config_file();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(cfg) = serde_json::from_str::<Config>(&content) {
            return cfg;
        }
    }
    let cfg = Config::default();
    let _ = write_json(&path, &cfg);
    cfg
}

/// 保存配置到 config.json
pub fn save_config(cfg: &Config) -> Result<(), String> {
    let _ = fs::create_dir_all(data_dir());
    write_json(&config_file(), cfg).map_err(|e| format!("保存配置失败：{}", e))
}

/// 原子写入 JSON 文件并 fsync
pub fn write_json<T: Serialize>(path: &Path, obj: &T) -> io::Result<()> {
    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    {
        let f = File::create(&tmp)?;
        let mut w = BufWriter::new(&f);
        serde_json::to_writer_pretty(&mut w, obj)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        w.flush()?;
        f.sync_data()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// 逐行写入器：每行写入后立即 flush + fsync，超过指定大小自动轮转
pub struct RotatingWriter {
    path: PathBuf,
    header: Option<String>,
    rotate_bytes: u64,
    file: Option<File>,
    written: u64,
}

impl RotatingWriter {
    pub fn new(path: PathBuf, header: Option<&str>, rotate_bytes: u64) -> Self {
        let mut rw = Self {
            path,
            header: header.map(|s| s.to_string()),
            rotate_bytes,
            file: None,
            written: 0,
        };
        rw.open();
        rw
    }

    fn open(&mut self) {
        let is_new = fs::metadata(&self.path).map(|m| m.len() == 0).unwrap_or(true);
        match OpenOptions::new().create(true).append(true).open(&self.path) {
            Ok(mut f) => {
                self.written = f.metadata().map(|m| m.len()).unwrap_or(0);
                if is_new {
                    if let Some(ref hdr) = self.header {
                        let _ = writeln!(f, "{}", hdr);
                        let _ = f.flush();
                        self.written += hdr.len() as u64 + 1;
                    }
                }
                self.file = Some(f);
            }
            Err(e) => {
                boot_log(&format!("[warn] open {} failed: {}", self.path.display(), e));
                self.file = None;
            }
        }
    }

    pub fn write(&mut self, line: &str) {
        if self.written > self.rotate_bytes {
            self.file.take();
            let bak = PathBuf::from(format!("{}.1", self.path.display()));
            let _ = fs::remove_file(&bak);
            let _ = fs::rename(&self.path, &bak);
            self.open();
        }
        let clean = line.trim_end_matches('\n');
        let mut need_reopen = false;
        if let Some(ref mut f) = self.file {
            // 整行一次性写入，减少读者看到半行的概率
            let mut buf = String::with_capacity(clean.len() + 1);
            buf.push_str(clean);
            buf.push('\n');
            if f.write_all(buf.as_bytes()).is_ok() && f.flush().is_ok() {
                let _ = f.sync_data();
                self.written += clean.len() as u64 + 1;
            } else {
                need_reopen = true;
            }
        } else {
            need_reopen = true;
        }
        if need_reopen {
            boot_log(&format!("[warn] write {} failed, reopening", self.path.display()));
            self.file.take();
            self.open();
        }
    }

    pub fn close(&mut self) {
        if let Some(mut f) = self.file.take() {
            let _ = f.flush();
            let _ = f.sync_data();
        }
    }
}

/// 将外部命令输出字节解码为文本，优先 UTF-8，回退 UTF-8 有损解码
pub fn decode_bytes(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// 无窗口执行外部命令并带超时强杀，返回 (返回码, 输出文本)
pub fn run_hidden(program: &str, args: &[&str], timeout: Duration) -> (i32, String) {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW);

    let mut child: Child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return (-1, format!("ERR {}", e)),
    };

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let out = child.wait_with_output();
                return match out {
                    Ok(o) => {
                        let mut combined = o.stdout;
                        combined.extend_from_slice(&o.stderr);
                        let rc = o.status.code().unwrap_or(-1);
                        (rc, decode_bytes(&combined))
                    }
                    Err(e) => (-1, format!("ERR {}", e)),
                };
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return (-9, "TIMEOUT".to_string());
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                let _ = child.kill();
                return (-1, format!("ERR {}", e));
            }
        }
    }
}

/// 单实例文件锁句柄（保持打开直到进程退出）
pub struct InstanceLock {
    _file: File,
}

/// 尝试获取单实例独占锁并写入 gpucap.pid
pub fn acquire_lock() -> Option<InstanceLock> {
    let dir = data_dir();
    let _ = fs::create_dir_all(&dir);
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(lock_file())
        .ok()?;

    let handle = file.as_raw_handle() as HANDLE;
    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        LockFileEx(
            handle,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            1,
            0,
            &mut overlapped,
        )
    };
    if ok == 0 {
        return None;
    }

    if let Ok(mut pf) = File::create(pid_file()) {
        let _ = write!(pf, "{}", std::process::id());
        let _ = pf.flush();
        let _ = pf.sync_data();
    }

    Some(InstanceLock { _file: file })
}

/// 通过内核接口检测指定 PID 是否存活
pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut code: u32 = 0;
        let ok = GetExitCodeProcess(h, &mut code);
        CloseHandle(h);
        ok != 0 && code == STILL_ACTIVE as u32
    }
}

/// 读取当前正在运行的采集器 PID
pub fn running_pid() -> Option<u32> {
    for path in [pid_file(), lock_file()] {
        if let Ok(txt) = fs::read_to_string(&path) {
            if let Ok(pid) = txt.trim().parse::<u32>() {
                if pid_alive(pid) {
                    return Some(pid);
                }
            }
        }
    }
    None
}

/// 在命令行模式下附加到父控制台以便打印标准输出（配合 windows 子系统消除双击黑框闪烁）
pub fn attach_parent_console_if_needed() {
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// 通过 EnumWindows 查找当前进程的主窗口句柄（有标题的顶层窗口），并用原生 API 显示或隐藏
pub fn set_main_window_visible(visible: bool) {
    struct Ctx {
        pid: u32,
        hwnd: windows_sys::Win32::Foundation::HWND,
    }
    unsafe extern "system" fn enum_cb(
        hwnd: windows_sys::Win32::Foundation::HWND,
        lparam: isize,
    ) -> i32 {
        let ctx = &mut *(lparam as *mut Ctx);
        let mut win_pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut win_pid);
        if win_pid == ctx.pid && GetWindowTextLengthW(hwnd) > 0 {
            ctx.hwnd = hwnd;
            return 0;
        }
        1
    }

    let mut ctx = Ctx {
        pid: std::process::id(),
        hwnd: std::ptr::null_mut(),
    };
    unsafe {
        EnumWindows(Some(enum_cb), (&mut ctx as *mut Ctx) as isize);
        if !ctx.hwnd.is_null() {
            if visible {
                ShowWindow(ctx.hwnd, SW_RESTORE);
                SetForegroundWindow(ctx.hwnd);
            } else {
                ShowWindow(ctx.hwnd, SW_HIDE);
            }
        }
    }
}

/// 界面单实例互斥锁与跨进程唤醒事件
pub struct UiSingleton {
    mutex: HANDLE,
    wakeup_event: HANDLE,
}

unsafe impl Send for UiSingleton {}
unsafe impl Sync for UiSingleton {}

impl UiSingleton {
    /// 尝试获取界面单实例锁：
    /// 若已有界面实例运行，则触发其唤醒事件（使其从托盘或后台恢复显示）并返回 None。
    pub fn acquire_or_wakeup() -> Option<Self> {
        let mutex_name = to_wide("Local\\GPUCap_UI_Singleton_v1");
        let event_name = to_wide("Local\\GPUCap_UI_Wakeup_v1");
        unsafe {
            let mutex = CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr());
            if mutex.is_null() {
                return None;
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(mutex);
                let ev = OpenEventW(EVENT_MODIFY_STATE, 0, event_name.as_ptr());
                if !ev.is_null() {
                    let _ = SetEvent(ev);
                    CloseHandle(ev);
                }
                return None;
            }
            let wakeup_event = CreateEventW(std::ptr::null(), 0, 0, event_name.as_ptr());
            Some(Self {
                mutex,
                wakeup_event,
            })
        }
    }

    /// 阻塞等待来自新启动实例的唤醒信号
    pub fn wait_wakeup(&self) -> bool {
        if self.wakeup_event.is_null() {
            return false;
        }
        unsafe { WaitForSingleObject(self.wakeup_event, INFINITE) == 0 }
    }
}

impl Drop for UiSingleton {
    fn drop(&mut self) {
        unsafe {
            if !self.wakeup_event.is_null() {
                CloseHandle(self.wakeup_event);
            }
            if !self.mutex.is_null() {
                CloseHandle(self.mutex);
            }
        }
    }
}

/// 将字符串转换为以空字符结尾的 UTF-16 宽字符序列
pub fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// 用系统外壳打开目录（ShellExecuteW）。
/// 不再用 Command 拉起 explorer.exe：界面进程释放控制台后，子进程继承到失效的标准句柄会导致启动失败。
pub fn open_folder_in_explorer(path: &Path) -> Result<(), String> {
    let _ = fs::create_dir_all(path);
    let wpath = to_wide(&path.to_string_lossy());
    let op = to_wide("open");
    let r = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            wpath.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if r as isize > 32 {
        Ok(())
    } else {
        Err(format!("打开目录失败（错误码 {}）：{}", r as isize, path.display()))
    }
}

/// 删除一个会话目录及其产生的事故取证目录，返回删除的事故目录数量。
/// 只允许删除 data\sessions 下的直接子目录，防止误删其它路径。
pub fn delete_session_data(session: &Path) -> Result<usize, String> {
    let base = fs::canonicalize(sessions_dir()).map_err(|e| format!("会话根目录不可用：{}", e))?;
    let target = fs::canonicalize(session).map_err(|e| format!("会话目录不存在：{}", e))?;
    if target.parent() != Some(base.as_path()) {
        return Err(format!("拒绝删除非会话目录：{}", session.display()));
    }

    // 找出属于该会话的事故目录（incident.json 的 session 字段指向它）
    let session_key = session.to_string_lossy().to_lowercase();
    let mut removed_incidents = 0;
    if let Ok(entries) = fs::read_dir(incidents_dir()) {
        for e in entries.flatten() {
            let dir = e.path();
            let Ok(raw) = fs::read_to_string(dir.join("incident.json")) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
                continue;
            };
            let owner = v.get("session").and_then(|s| s.as_str()).unwrap_or("");
            if owner.to_lowercase() == session_key && fs::remove_dir_all(&dir).is_ok() {
                removed_incidents += 1;
            }
        }
    }

    fs::remove_dir_all(&target).map_err(|e| format!("删除会话目录失败：{}", e))?;
    Ok(removed_incidents)
}

/// 读取 latest.txt 指向的会话目录
pub fn latest_session_path() -> Option<PathBuf> {
    let raw = fs::read_to_string(latest_file()).ok()?;
    let p = PathBuf::from(raw.trim());
    if p.as_os_str().is_empty() {
        None
    } else {
        Some(p)
    }
}

/// 从系统注册表读取页文件配置（REG_MULTI_SZ），无需调用 PowerShell
pub fn read_pagefile_from_registry() -> String {
    let subkey = to_wide(r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management");
    let val_name = to_wide("PagingFiles");
    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, subkey.as_ptr(), 0, KEY_READ, &mut hkey)
            != ERROR_SUCCESS
        {
            return String::new();
        }
        let mut dtype: u32 = 0;
        let mut size: u32 = 0;
        let rc = RegQueryValueExW(
            hkey,
            val_name.as_ptr(),
            std::ptr::null(),
            &mut dtype,
            std::ptr::null_mut(),
            &mut size,
        );
        if rc != ERROR_SUCCESS || size == 0 {
            RegCloseKey(hkey);
            return String::new();
        }
        let mut buf = vec![0u16; (size as usize + 1) / 2];
        let rc2 = RegQueryValueExW(
            hkey,
            val_name.as_ptr(),
            std::ptr::null(),
            &mut dtype,
            buf.as_mut_ptr() as *mut u8,
            &mut size,
        );
        RegCloseKey(hkey);
        if rc2 != ERROR_SUCCESS {
            return String::new();
        }
        let items: Vec<String> = buf
            .split(|&c| c == 0)
            .filter(|s| !s.is_empty())
            .map(String::from_utf16_lossy)
            .collect();
        items.join("; ")
    }
}

/// 将指定会话目录及关联事故目录打包导出为 zip 压缩包
pub fn export_session_zip(session: &Path) -> Result<PathBuf, String> {
    let name = session
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "session".to_string());
    let out_path = data_dir().join(format!("export_{}.zip", name));
    let file = File::create(&out_path).map_err(|e| format!("创建压缩包失败：{}", e))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    add_dir_to_zip(&mut zip, session, &format!("session_{}", name), opts)?;

    let session_key = session.to_string_lossy().to_lowercase();
    if let Ok(entries) = fs::read_dir(incidents_dir()) {
        for e in entries.flatten() {
            let dir = e.path();
            if !dir.is_dir() {
                continue;
            }
            let Ok(raw) = fs::read_to_string(dir.join("incident.json")) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
                continue;
            };
            let owner = v.get("session").and_then(|s| s.as_str()).unwrap_or("");
            if owner.to_lowercase() == session_key {
                let inc_name = dir
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "incident".to_string());
                let _ = add_dir_to_zip(&mut zip, &dir, &format!("incidents/{}", inc_name), opts);
            }
        }
    }

    zip.finish().map_err(|e| format!("完成压缩包失败：{}", e))?;
    Ok(out_path)
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<File>,
    dir: &Path,
    prefix: &str,
    opts: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let fname = entry.file_name().to_string_lossy().into_owned();
        if fname.ends_with(".tmp") {
            continue;
        }
        let zip_name = format!("{}/{}", prefix, fname);
        if p.is_dir() {
            add_dir_to_zip(zip, &p, &zip_name, opts)?;
        } else if p.is_file() {
            if zip.start_file(&zip_name, opts).is_ok() {
                if let Ok(mut f) = File::open(&p) {
                    let _ = io::copy(&mut f, zip);
                }
            }
        }
    }
    Ok(())
}

const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const STARTUP_APPROVED_PATH: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const RUN_VALUE_NAME: &str = "GPUCap";

/// 读取当前注册表 Run 项的命令行字符串
pub fn read_startup_reg_cmd() -> Option<String> {
    let subkey = to_wide(RUN_KEY_PATH);
    let val_name = to_wide(RUN_VALUE_NAME);
    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) != ERROR_SUCCESS
        {
            return None;
        }
        let mut dtype: u32 = 0;
        let mut size: u32 = 0;
        let rc = RegQueryValueExW(
            hkey,
            val_name.as_ptr(),
            std::ptr::null(),
            &mut dtype,
            std::ptr::null_mut(),
            &mut size,
        );
        if rc != ERROR_SUCCESS || size == 0 {
            RegCloseKey(hkey);
            return None;
        }
        let mut buf = vec![0u16; (size as usize / 2) + 1];
        let rc2 = RegQueryValueExW(
            hkey,
            val_name.as_ptr(),
            std::ptr::null(),
            &mut dtype,
            buf.as_mut_ptr() as *mut u8,
            &mut size,
        );
        RegCloseKey(hkey);
        if rc2 != ERROR_SUCCESS {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }
}

/// 检查当前用户注册表是否已配置开机自启（且未被任务管理器禁用）
pub fn is_startup_installed() -> bool {
    if read_startup_reg_cmd().is_none() {
        return false;
    }
    let subkey = to_wide(STARTUP_APPROVED_PATH);
    let val_name = to_wide(RUN_VALUE_NAME);
    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) != ERROR_SUCCESS
        {
            return true;
        }
        let mut dtype: u32 = 0;
        let mut buf = [0u8; 16];
        let mut size: u32 = buf.len() as u32;
        let rc = RegQueryValueExW(
            hkey,
            val_name.as_ptr(),
            std::ptr::null(),
            &mut dtype,
            buf.as_mut_ptr(),
            &mut size,
        );
        RegCloseKey(hkey);
        if rc == ERROR_SUCCESS && size >= 1 {
            // Windows StartupApproved 首字节奇数（如 0x03）表示被禁用
            return (buf[0] & 1) == 0;
        }
        true
    }
}

/// 通过注册表安装当前用户开机自启（静默托盘模式 + 写入任务管理器启用标志）
pub fn install_startup_reg() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("无法定位可执行文件: {}", e))?;
    let cmd = format!("\"{}\" ui --minimized", exe.display());
    let subkey = to_wide(RUN_KEY_PATH);
    let val_name = to_wide(RUN_VALUE_NAME);
    let val_data = to_wide(&cmd);
    let byte_len = (val_data.len() * 2) as u32;
    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        let rc = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_WRITE,
            std::ptr::null(),
            &mut hkey,
            std::ptr::null_mut(),
        );
        if rc != ERROR_SUCCESS {
            return Err(format!("打开注册表失败，错误码 {}", rc));
        }
        let rc2 = RegSetValueExW(
            hkey,
            val_name.as_ptr(),
            0,
            REG_SZ,
            val_data.as_ptr() as *const u8,
            byte_len,
        );
        RegCloseKey(hkey);
        if rc2 != ERROR_SUCCESS {
            return Err(format!("写入注册表失败，错误码 {}", rc2));
        }

        // 显式写入 StartupApproved\Run 启用标志（12 字节 0x02...），确保任务管理器显示已启用
        let app_sub = to_wide(STARTUP_APPROVED_PATH);
        let mut h_app: HKEY = std::ptr::null_mut();
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            app_sub.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_WRITE,
            std::ptr::null(),
            &mut h_app,
            std::ptr::null_mut(),
        ) == ERROR_SUCCESS
        {
            let approved: [u8; 12] = [0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            let _ = RegSetValueExW(
                h_app,
                val_name.as_ptr(),
                0,
                REG_BINARY,
                approved.as_ptr(),
                approved.len() as u32,
            );
            RegCloseKey(h_app);
        }
    }
    Ok(())
}

/// 从注册表移除开机自启（并清理 StartupApproved 与旧版 GPUCap.cmd）
pub fn uninstall_startup_reg() -> Result<(), String> {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let legacy = PathBuf::from(appdata)
            .join(r"Microsoft\Windows\Start Menu\Programs\Startup")
            .join("GPUCap.cmd");
        let _ = fs::remove_file(legacy);
    }
    let subkey = to_wide(RUN_KEY_PATH);
    let val_name = to_wide(RUN_VALUE_NAME);
    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        let rc = RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_WRITE, &mut hkey);
        if rc != ERROR_SUCCESS {
            return Err(format!("打开注册表失败，错误码 {}", rc));
        }
        let rc2 = RegDeleteValueW(hkey, val_name.as_ptr());
        RegCloseKey(hkey);
        if rc2 != ERROR_SUCCESS && rc2 != 2 {
            return Err(format!("移除注册表项失败，错误码 {}", rc2));
        }

        let app_sub = to_wide(STARTUP_APPROVED_PATH);
        let mut h_app: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, app_sub.as_ptr(), 0, KEY_WRITE, &mut h_app)
            == ERROR_SUCCESS
        {
            let _ = RegDeleteValueW(h_app, val_name.as_ptr());
            RegCloseKey(h_app);
        }
    }
    Ok(())
}
