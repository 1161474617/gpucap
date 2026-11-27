//! 采集守护进程、事故取证与命令行子命令模块。

use crate::collectors::{EventTailer, GpuSampler, MemSampler, ProcSampler, WerWatcher};
use crate::infra::{
    acquire_lock, boot_log, config_file, data_dir, incidents_dir, install_startup_reg,
    latest_file, load_config, now_iso, now_iso_sec, pid_file, read_pagefile_from_registry,
    run_hidden, running_pid, sessions_dir, stop_file, uninstall_startup_reg, write_json, Config,
    RotatingWriter, APP_NAME, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, DETACHED_PROCESS,
};
use chrono::{DateTime, Local};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows_sys::Win32::System::SystemInformation::GetTickCount64;

pub const GPU_HEADER: &str = "ts,temp_c,power_w,power_limit_w,core_mhz,mem_mhz,pcie_gen,pcie_width,util_gpu,util_mem,mem_used_mb,mem_total_mb,fan_pct,throttle,ok,latency_ms,extra,hotspot_c";
pub const MEM_HEADER: &str = "ts,commit_gb,commit_limit_gb,phys_avail_mb,phys_total_mb,sys_cache_mb,kernel_paged_mb,kernel_nonpaged_mb";
pub const EVT_HEADER: &str = "ts,log,level,id,provider,record_id,message";
pub const WER_HEADER: &str = "ts,kind,name,created,action";
pub const PROC_HEADER: &str = "ts,name,pid,ws_mb,cpu_s";

const WATCH_IDS: &[i64] = &[
    41, 1001, 4101, 4102, 117, 219, 6008, 6005, 6006, 7000, 153, 129, 1, 2, 13, 14,
];
const CRITICAL_IDS: &[i64] = &[41, 1001, 4101, 4102, 117, 219, 6008];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Counters {
    pub gpu: u64,
    pub smi_fail: u64,
    pub events: u64,
    pub wer: u64,
    pub incidents: u64,
}

/// 事故取证执行器
pub struct Incident<'a> {
    session_dir: &'a Path,
    gpu_exe: &'a str,
    wer_bases: &'a [PathBuf],
    wer_live: &'a Path,
}

impl<'a> Incident<'a> {
    pub fn capture(&self, reason: &str, detail: &str, extra: Value) -> (PathBuf, Value) {
        let ts = Local::now().format("%Y%m%d-%H%M%S").to_string();
        let mut dir = incidents_dir().join(&ts);
        if fs::create_dir_all(&dir).is_err() {
            dir = self.session_dir.join("incidents");
            let _ = fs::create_dir_all(&dir);
        }

        let mut info = json!({
            "time": now_iso(),
            "reason": reason,
            "detail": detail,
            "session": self.session_dir.to_string_lossy(),
            "extra": extra,
        });

        // 1) 最近显卡采样摘要与峰值
        let tail = self.tail_gpu(120);
        if let Some(last) = tail.last() {
            let max_temp = tail
                .iter()
                .map(|r| parse_f64(&r.temp_c))
                .fold(0.0_f64, f64::max);
            let max_power = tail
                .iter()
                .map(|r| parse_f64(&r.power_w))
                .fold(0.0_f64, f64::max);
            let max_vram = tail
                .iter()
                .map(|r| parse_f64(&r.mem_used_mb))
                .fold(0.0_f64, f64::max);
            if let Some(obj) = info.as_object_mut() {
                obj.insert(
                    "gpu_last".to_string(),
                    json!({
                        "ts": last.ts,
                        "temp_c": last.temp_c,
                        "power_w": last.power_w,
                        "mem_used_mb": last.mem_used_mb,
                        "pcie_gen": last.pcie_gen,
                        "pcie_width": last.pcie_width,
                    }),
                );
                obj.insert(
                    "gpu_peak".to_string(),
                    json!({
                        "temp": max_temp,
                        "power": max_power,
                        "vram": max_vram,
                    }),
                );
            }
        }

        // 2) nvidia-smi -q 全量快照
        let (rc, out) = run_hidden(self.gpu_exe, &["-q"], Duration::from_secs(20));
        if rc == 0 {
            let _ = fs::write(dir.join("nvidia-smi-q.txt"), out);
        } else if let Some(obj) = info.as_object_mut() {
            obj.insert("smi_error".to_string(), json!(format!("rc={}", rc)));
        }

        // 3) WER 与 LiveKernelReports 目录清单
        let mut lines = Vec::new();
        for b in self.wer_bases {
            lines.push(format!("== {}", b.display()));
            match fs::read_dir(b) {
                Ok(entries) => {
                    let mut items: Vec<(String, PathBuf)> = entries
                        .flatten()
                        .filter(|e| e.path().is_dir())
                        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                        .collect();
                    items.sort_by(|a, b| a.0.cmp(&b.0));
                    for (name, p) in items {
                        let c = fs::metadata(&p)
                            .and_then(|m| m.created())
                            .map(|st| {
                                let dt: DateTime<Local> = st.into();
                                dt.format("%Y-%m-%dT%H:%M:%S").to_string()
                            })
                            .unwrap_or_else(|_| "?".to_string());
                        lines.push(format!("  {}  {}", c, name));
                    }
                }
                Err(e) => {
                    lines.push(format!("  <denied: {}>", e));
                }
            }
        }
        lines.push(format!("== {}", self.wer_live.display()));
        match fs::read_dir(self.wer_live) {
            Ok(_) => {
                let mut files = Vec::new();
                collect_files_recursive(self.wer_live, &mut files);
                for f in files {
                    lines.push(format!("  {}", f.display()));
                }
            }
            Err(e) => {
                lines.push(format!("  <denied: {}>", e));
            }
        }
        let _ = fs::write(dir.join("wer-list.txt"), lines.join("\n"));

        // 4) 尝试拷贝 Kernel_* 下的 Report.wer
        for b in self.wer_bases {
            if let Ok(entries) = fs::read_dir(b) {
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if !name.to_lowercase().starts_with("kernel_") {
                        continue;
                    }
                    let src = e.path().join("Report.wer");
                    if src.is_file() {
                        let tail_name: String = name
                            .chars()
                            .rev()
                            .take(16)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect();
                        let dst = dir.join(format!("Report_{}.wer", tail_name));
                        let _ = fs::copy(&src, &dst);
                    }
                }
            }
        }

        let _ = write_json(&dir.join("incident.json"), &info);
        (dir, info)
    }

    fn tail_gpu(&self, n: usize) -> Vec<GpuTailRow> {
        let path = self.session_dir.join("gpu.csv");
        let f = match File::open(path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        let reader = BufReader::new(f);
        let all: Vec<String> = reader.lines().map_while(Result::ok).collect();
        let start = all.len().saturating_sub(n);
        let slice = &all[start..];
        let mut out = Vec::new();
        for line in slice.iter().skip(1) {
            let parts: Vec<&str> = line.trim().split(',').collect();
            if parts.len() < 16 {
                continue;
            }
            out.push(GpuTailRow {
                ts: parts[0].to_string(),
                temp_c: parts[1].to_string(),
                power_w: parts[2].to_string(),
                mem_used_mb: parts[10].to_string(),
                pcie_gen: parts[6].to_string(),
                pcie_width: parts[7].to_string(),
            });
        }
        out
    }
}

struct GpuTailRow {
    ts: String,
    temp_c: String,
    power_w: String,
    mem_used_mb: String,
    pcie_gen: String,
    pcie_width: String,
}

fn collect_files_recursive(root: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(root) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_files_recursive(&p, out);
            } else if p.is_file() {
                out.push(p);
            }
        }
    }
}

fn parse_f64(s: &str) -> f64 {
    s.trim().parse::<f64>().unwrap_or(0.0)
}

/// 主采集守护对象
pub struct Daemon {
    cfg: Config,
    minutes: Option<f64>,
    started: Instant,
    session_dir: PathBuf,
    w_gpu: Option<RotatingWriter>,
    w_mem: Option<RotatingWriter>,
    w_events: Option<RotatingWriter>,
    w_wer: Option<RotatingWriter>,
    w_proc: Option<RotatingWriter>,
    w_alerts: Option<RotatingWriter>,
    gpu: GpuSampler,
    mem: MemSampler,
    proc: ProcSampler,
    events: EventTailer,
    wer: WerWatcher,
    watch_re: Regex,
    watch_ids: HashSet<i64>,
    critical_ids: HashSet<i64>,
    smi_fail_streak: u64,
    game_present: Option<bool>,
    game_lost_at: Option<Instant>,
    last_load_power: f64,
    counters: Counters,
    rot_bytes: u64,
}

impl Daemon {
    pub fn new(cfg: Config, minutes: Option<f64>) -> Self {
        let rot_bytes = cfg.rotate_mb.saturating_mul(1024 * 1024);
        let proc = ProcSampler::new(&cfg.game_names);
        let gpu = GpuSampler::new(cfg.gpu_index);
        Self {
            cfg,
            minutes,
            started: Instant::now(),
            session_dir: PathBuf::new(),
            w_gpu: None,
            w_mem: None,
            w_events: None,
            w_wer: None,
            w_proc: None,
            w_alerts: None,
            gpu,
            mem: MemSampler,
            proc,
            events: EventTailer::new(),
            wer: WerWatcher::new(),
            watch_re: Regex::new(
                r"(?i)WHEA-Logger|nvlddmkm|Display|Kernel-Power|BugCheck|Kernel-PnP|Disk|amdkmpfd|amdwddmg|LiveKernelEvent",
            )
            .expect("内置正则有效"),
            watch_ids: WATCH_IDS.iter().copied().collect(),
            critical_ids: CRITICAL_IDS.iter().copied().collect(),
            smi_fail_streak: 0,
            game_present: None,
            game_lost_at: None,
            last_load_power: 0.0,
            counters: Counters::default(),
            rot_bytes,
        }
    }

    fn open_session(&mut self) {
        let sdir = sessions_dir();
        let _ = fs::create_dir_all(&sdir);
        let ts = Local::now().format("%Y%m%d-%H%M%S").to_string();
        self.session_dir = sdir.join(ts);
        let _ = fs::create_dir_all(&self.session_dir);
        let _ = fs::write(latest_file(), self.session_dir.to_string_lossy().as_bytes());

        self.w_gpu = Some(RotatingWriter::new(
            self.session_dir.join("gpu.csv"),
            Some(GPU_HEADER),
            self.rot_bytes,
        ));
        self.w_mem = Some(RotatingWriter::new(
            self.session_dir.join("mem.csv"),
            Some(MEM_HEADER),
            self.rot_bytes,
        ));
        self.w_events = Some(RotatingWriter::new(
            self.session_dir.join("events.csv"),
            Some(EVT_HEADER),
            self.rot_bytes,
        ));
        self.w_wer = Some(RotatingWriter::new(
            self.session_dir.join("wer.csv"),
            Some(WER_HEADER),
            self.rot_bytes,
        ));
        self.w_proc = Some(RotatingWriter::new(
            self.session_dir.join("proc.csv"),
            Some(PROC_HEADER),
            self.rot_bytes,
        ));
        self.w_alerts = Some(RotatingWriter::new(
            self.session_dir.join("alerts.log"),
            None,
            self.rot_bytes,
        ));

        let info = json!({
            "app": APP_NAME,
            "version": 2,
            "impl": "rust",
            "host": std::env::var("COMPUTERNAME").unwrap_or_default(),
            "user": std::env::var("USERNAME").unwrap_or_default(),
            "start": now_iso(),
            "session_dir": self.session_dir.to_string_lossy(),
            "game_names": self.cfg.game_names,
            "intervals": {
                "gpu_interval": self.cfg.gpu_interval,
                "mem_interval": self.cfg.mem_interval,
                "proc_interval": self.cfg.proc_interval,
                "wer_interval": self.cfg.wer_interval,
            },
            "boot_time": Self::boot_time(),
            "pagefile": Self::pagefile(),
            "gpu_static": self.gpu.static_query(),
            "os": std::env::consts::OS,
        });
        let _ = write_json(&self.session_dir.join("session.json"), &info);
        boot_log(&format!("[info] session {}", self.session_dir.display()));
    }

    fn boot_time() -> String {
        let up_ms = unsafe { GetTickCount64() };
        let boot_sys = SystemTime::now()
            .checked_sub(Duration::from_millis(up_ms))
            .unwrap_or(SystemTime::now());
        let dt: DateTime<Local> = boot_sys.into();
        dt.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
    }

    fn pagefile() -> String {
        read_pagefile_from_registry()
    }

    fn heartbeat(&self) {
        // 先写临时文件再原子替换，界面读取时永远不会读到被截断的空文件
        let tmp = self.session_dir.join("heartbeat.txt.tmp");
        if fs::write(&tmp, now_iso()).is_ok() {
            let _ = fs::rename(&tmp, self.session_dir.join("heartbeat.txt"));
        }
    }

    fn alert(&mut self, kind: &str, detail: &str) {
        let line = format!("{} [{}] {}", now_iso(), kind, detail);
        if let Some(ref mut w) = self.w_alerts {
            w.write(&line);
        }
        let snippet: String = detail.chars().take(200).collect();
        boot_log(&format!("[alert] {} {}", kind, snippet));
    }

    fn incident(&mut self, reason: &str, detail: &str, extra: Value) {
        let inc = Incident {
            session_dir: &self.session_dir,
            gpu_exe: &self.gpu.exe,
            wer_bases: &self.wer.bases,
            wer_live: &self.wer.live_dir,
        };
        let (dir, _) = inc.capture(reason, detail, extra);
        self.counters.incidents += 1;
        if let Some(ref mut w) = self.w_alerts {
            w.write(&format!(
                "{} [INCIDENT] {} -> {}",
                now_iso(),
                reason,
                dir.display()
            ));
        }
    }

    fn step_gpu(&mut self) {
        let (res, ms) = self.gpu.sample();
        let ts = now_iso();
        match res {
            Err(err) => {
                self.counters.smi_fail += 1;
                self.smi_fail_streak += 1;
                if let Some(ref mut w) = self.w_gpu {
                    w.write(&format!("{},,,,,,,,,,,,,FAIL,{},{}", ts, ms, err));
                }
                let s = self.smi_fail_streak;
                if s == 3 || s == 10 || s == 30 || (s > 30 && s % 60 == 0) {
                    self.alert("SMI_FAIL", &format!("nvidia-smi 连续 {} 次失败：{}", s, err));
                    self.incident("SMI_FAIL", &format!("streak={}", s), json!({ "err": err }));
                }
            }
            Ok(rec) => {
                self.smi_fail_streak = 0;
                self.counters.gpu += 1;
                let g = |k: &str| rec.get(k).map(|s| s.as_str()).unwrap_or("");
                let extra_str = self
                    .gpu
                    .extra
                    .iter()
                    .map(|k| format!("{}={}", k, g(k)))
                    .collect::<Vec<_>>()
                    .join("|");
                let line = [
                    ts.as_str(),
                    g("temperature.gpu"),
                    g("power.draw"),
                    g("power.limit"),
                    g("clocks.current.graphics"),
                    g("clocks.current.memory"),
                    g("pcie.link.gen.current"),
                    g("pcie.link.width.current"),
                    g("utilization.gpu"),
                    g("utilization.memory"),
                    g("memory.used"),
                    g("memory.total"),
                    g("fan.speed"),
                    g("clocks_throttle_reasons.active"),
                    "OK",
                    &ms.to_string(),
                    &extra_str,
                    g("temperature.hotspot"),
                ]
                .join(",");
                if let Some(ref mut w) = self.w_gpu {
                    w.write(&line);
                }

                self.last_load_power = parse_f64(g("power.draw"));
                let temp = parse_f64(g("temperature.gpu"));
                let hotspot = parse_f64(g("temperature.hotspot"));
                let pcie_gen = parse_f64(g("pcie.link.gen.current"));
                let thrott = g("clocks_throttle_reasons.active").to_string();

                if temp > 0.0 && temp >= self.cfg.temp_warn {
                    self.alert("TEMP", &format!("{}°C ≥ {}°C", temp, self.cfg.temp_warn));
                }
                if hotspot > 0.0 && hotspot >= self.cfg.hotspot_warn {
                    self.alert(
                        "HOTSPOT",
                        &format!("热点 {:.1}°C ≥ {:.1}°C", hotspot, self.cfg.hotspot_warn),
                    );
                }
                if temp > 0.0 && hotspot > temp {
                    let delta = hotspot - temp;
                    if delta >= self.cfg.hotspot_delta_warn {
                        self.alert(
                            "HOTSPOT_DELTA",
                            &format!(
                                "温差 {:.1}°C ≥ {:.1}°C (热点 {:.1}°C / 核心 {:.1}°C)",
                                delta, self.cfg.hotspot_delta_warn, hotspot, temp
                            ),
                        );
                    }
                }
                if self.last_load_power >= self.cfg.power_warn {
                    self.alert(
                        "POWER",
                        &format!("{}W ≥ {}W", self.last_load_power, self.cfg.power_warn),
                    );
                }
                if pcie_gen > 0.0 && self.last_load_power > 150.0 && pcie_gen < 4.0 {
                    self.alert(
                        "PCIE",
                        &format!(
                            "负载 {:.0}W 时链路降到 Gen{:.0}",
                            self.last_load_power, pcie_gen
                        ),
                    );
                }
                let hex_str = thrott.trim().trim_start_matches("0x").trim_start_matches("0X");
                if let Ok(t) = u64::from_str_radix(hex_str, 16) {
                    if (t & 0xEC) != 0 {
                        self.alert("THROTTLE", &format!("降频位 {}", thrott));
                    }
                }
            }
        }
    }

    fn step_mem(&mut self) {
        let Some(m) = self.mem.sample() else {
            return;
        };
        let line = format!(
            "{},{},{},{},{},{},{},{}",
            now_iso(),
            m.commit_gb,
            m.commit_limit_gb,
            m.phys_avail_mb,
            m.phys_total_mb,
            m.sys_cache_mb,
            m.kernel_paged_mb,
            m.kernel_nonpaged_mb
        );
        if let Some(ref mut w) = self.w_mem {
            w.write(&line);
        }
        if m.commit_limit_gb > 0.0 {
            let ratio = m.commit_gb / m.commit_limit_gb;
            if ratio > 0.97 {
                self.alert(
                    "COMMIT",
                    &format!(
                        "提交量 {}GB / {}GB ({:.0}%)",
                        m.commit_gb,
                        m.commit_limit_gb,
                        ratio * 100.0
                    ),
                );
            }
        }
    }

    fn step_proc(&mut self) {
        let found = self.proc.sample();
        let ts = now_iso();
        if !found.is_empty() {
            for p in &found {
                let line = format!("{},{},{},{},{}", ts, p.name, p.pid, p.ws_mb, p.cpu_s);
                if let Some(ref mut w) = self.w_proc {
                    w.write(&line);
                }
            }
            if self.game_present == Some(false) {
                self.alert("GAME", &format!("游戏重新启动 pid={}", found[0].pid));
            }
            self.game_present = Some(true);
        } else {
            if let Some(ref mut w) = self.w_proc {
                w.write(&format!("{},-,-,0,-", ts));
            }
            if self.game_present == Some(true) {
                self.game_present = Some(false);
                let lost = Instant::now();
                self.game_lost_at = Some(lost);
                if self.last_load_power > 150.0 || lost.elapsed().as_secs_f64() < 15.0 {
                    self.alert(
                        "GAME_EXIT",
                        &format!(
                            "游戏进程消失（此前功耗 {:.0}W），10 秒内无 WER 则可能是闪退",
                            self.last_load_power
                        ),
                    );
                    self.incident(
                        "GAME_EXIT_UNEXPECTED",
                        &format!("power_before={}", self.last_load_power),
                        json!({}),
                    );
                }
            }
        }
    }

    fn step_events(&mut self) {
        let evs = self.events.poll(Duration::from_millis(200));
        for e in evs {
            let prov = e.get("prov").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let eid = e.get("id").and_then(|v| v.as_i64()).unwrap_or(-1);
            let level = e.get("level").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let msg = e
                .get("msg")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .replace('\n', " ");
            let ts = e
                .get("ts")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(now_iso);
            let log_name = e.get("log").and_then(|v| v.as_str()).unwrap_or("");
            let rid = e
                .get("rid")
                .map(|v| match v {
                    Value::Null => String::new(),
                    _ => v.to_string(),
                })
                .unwrap_or_default();

            let interesting = self.watch_re.is_match(&prov)
                || self.watch_ids.contains(&eid)
                || level == "Error"
                || level == "Critical";

            let msg_clean: String = msg.replace('"', "'").chars().take(400).collect();
            let eid_str = if eid >= 0 { eid.to_string() } else { String::new() };
            let line = format!(
                "{},{},{},{},{},{},\"{}\"",
                ts,
                log_name,
                level,
                eid_str,
                prov.replace(',', " "),
                rid,
                msg_clean
            );
            if let Some(ref mut w) = self.w_events {
                w.write(&line);
            }
            self.counters.events += 1;

            if interesting {
                let snippet160: String = msg.chars().take(160).collect();
                self.alert(
                    "EVENT",
                    &format!("{} id={} {}: {}", level, eid_str, prov, snippet160),
                );
                if self.critical_ids.contains(&eid) {
                    let snippet300: String = msg.chars().take(300).collect();
                    self.incident(
                        &format!("EVENT_{}", eid),
                        &snippet300,
                        json!({
                            "provider": prov,
                            "record_id": e.get("rid"),
                            "level": level,
                        }),
                    );
                }
            }
        }
    }

    fn step_wer(&mut self) {
        let items = self.wer.poll();
        for item in items {
            let line = format!(
                "{},{},{},{},logged",
                now_iso(),
                item.kind,
                item.name.replace(',', " "),
                item.created
            );
            if let Some(ref mut w) = self.w_wer {
                w.write(&line);
            }
            self.counters.wer += 1;
            self.alert(
                "WER_NEW",
                &format!("{} {} created={}", item.kind, item.name, item.created),
            );
            if item.kind == "KERNEL_REPORT"
                || item.kind == "LIVEDUMP"
                || item.name.to_lowercase().starts_with("kernel_")
            {
                let extra = serde_json::to_value(&item).unwrap_or_else(|_| json!({}));
                self.incident(&format!("WER_{}", item.kind), &item.name, extra);
            }
        }
    }

    pub fn run(&mut self) {
        // 用户显式启动时清除上一次的 STOP 标记
        let _ = fs::remove_file(stop_file());
        self.open_session();
        boot_log(&format!(
            "[info] {} start session={}",
            APP_NAME,
            self.session_dir.display()
        ));
        boot_log(&format!(
            "[info] gpu fields={} extra={:?}",
            self.gpu.fields.len(),
            self.gpu.extra
        ));

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_c = Arc::clone(&stop_flag);
        let _ = ctrlc::set_handler(move || {
            stop_flag_c.store(true, Ordering::SeqCst);
        });

        // 45 秒主循环看门狗
        let last_tick_sec = Arc::new(AtomicU64::new(unix_secs()));
        let tick_watch = Arc::clone(&last_tick_sec);
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(5));
            let now = unix_secs();
            let prev = tick_watch.load(Ordering::Relaxed);
            if now.saturating_sub(prev) > 45 {
                boot_log("[fatal] watchdog: 主循环卡死超过 45s，强制退出以便守护进程重启");
                std::process::exit(4);
            }
        });

        let deadline = self
            .minutes
            .map(|m| self.started + Duration::from_secs_f64((m * 60.0).max(0.0)));

        let mut next_gpu = Instant::now();
        let mut next_mem = Instant::now();
        let mut next_proc = Instant::now();
        let mut next_wer = Instant::now();
        let mut session_started = Instant::now();
        let max_session_dur = if self.cfg.max_session_hours > 0 {
            Some(Duration::from_secs(self.cfg.max_session_hours * 3600))
        } else {
            None
        };

        loop {
            if stop_flag.load(Ordering::SeqCst) {
                boot_log("[info] Ctrl+C received");
                break;
            }
            if let Some(dl) = deadline {
                if Instant::now() >= dl {
                    boot_log("[info] reached --minutes limit");
                    break;
                }
            }
            if stop_file().exists() {
                boot_log("[info] STOP file detected");
                break;
            }
            if let Some(max_dur) = max_session_dur {
                if session_started.elapsed() >= max_dur {
                    boot_log("[info] max_session_hours reached, rotating session");
                    self.close_session_files();
                    self.counters = Counters::default();
                    self.open_session();
                    session_started = Instant::now();
                }
            }

            let now = Instant::now();
            if now >= next_gpu {
                next_gpu = now + Duration::from_secs_f64(self.cfg.gpu_interval.max(0.2));
                self.step_gpu();
            }
            if now >= next_mem {
                next_mem = now + Duration::from_secs_f64(self.cfg.mem_interval.max(0.5));
                self.step_mem();
            }
            if now >= next_proc {
                next_proc = now + Duration::from_secs_f64(self.cfg.proc_interval.max(0.5));
                self.step_proc();
            }
            if now >= next_wer {
                next_wer = now + Duration::from_secs_f64(self.cfg.wer_interval.max(0.5));
                self.step_wer();
            }
            self.step_events();

            self.heartbeat();
            last_tick_sec.store(unix_secs(), Ordering::Relaxed);

            let wait = next_gpu
                .saturating_duration_since(Instant::now())
                .clamp(Duration::from_millis(20), Duration::from_millis(500));
            thread::sleep(wait);
        }

        self.shutdown();
    }

    fn close_session_files(&mut self) {
        if let Some(ref mut w) = self.w_gpu.take() {
            w.close();
        }
        if let Some(ref mut w) = self.w_mem.take() {
            w.close();
        }
        if let Some(ref mut w) = self.w_events.take() {
            w.close();
        }
        if let Some(ref mut w) = self.w_wer.take() {
            w.close();
        }
        if let Some(ref mut w) = self.w_proc.take() {
            w.close();
        }
        if let Some(ref mut w) = self.w_alerts.take() {
            w.close();
        }
        let sjson = self.session_dir.join("session.json");
        if let Ok(raw) = fs::read_to_string(&sjson) {
            if let Ok(mut val) = serde_json::from_str::<Value>(&raw) {
                if let Some(obj) = val.as_object_mut() {
                    obj.insert("stop".to_string(), json!(now_iso()));
                    obj.insert(
                        "counters".to_string(),
                        serde_json::to_value(&self.counters).unwrap_or_else(|_| json!({})),
                    );
                }
                let _ = write_json(&sjson, &val);
            }
        }
    }

    fn shutdown(&mut self) {
        boot_log("[info] shutdown");
        self.events.close();
        self.close_session_files();
        let _ = fs::remove_file(pid_file());
    }
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// --------------------------------------------------------------------------- 子命令入口

pub fn cmd_run(minutes: Option<f64>) -> i32 {
    let Some(_lock) = acquire_lock() else {
        println!("已有一个 GPUCap 实例在运行（lock 被占用）。用 `status` 查看，或 `stop` 结束它。");
        return 2;
    };
    let cfg = load_config();
    let mut daemon = Daemon::new(cfg, minutes);
    daemon.run();
    0
}

pub fn cmd_daemon(minutes: Option<f64>) -> i32 {
    if let Some(pid) = running_pid() {
        println!("已在运行 pid={}", pid);
        return 0;
    }
    let Ok(exe) = std::env::current_exe() else {
        println!("无法定位当前可执行文件路径");
        return 1;
    };
    let mut args = vec!["run".to_string()];
    if let Some(m) = minutes {
        args.push("--minutes".to_string());
        args.push(m.to_string());
    }
    let child = Command::new(&exe)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        .spawn();

    match child {
        Ok(c) => {
            thread::sleep(Duration::from_millis(1500));
            let pid = running_pid().unwrap_or(c.id());
            println!("已启动后台采集，pid={}  数据目录：{}", pid, data_dir().display());
            0
        }
        Err(e) => {
            println!("启动后台采集失败：{}", e);
            1
        }
    }
}

pub fn cmd_stop() -> i32 {
    let pid = running_pid();
    let _ = fs::create_dir_all(data_dir());
    let _ = File::create(stop_file());
    boot_log("[info] STOP requested");
    let Some(pid) = pid else {
        println!("没有检测到运行中的实例（已写入 STOP，清理残留）。");
        return 0;
    };
    for _ in 0..20 {
        thread::sleep(Duration::from_millis(250));
        if running_pid().is_none() {
            println!("已停止 pid={}", pid);
            return 0;
        }
    }
    println!("实例 pid={} 未在 5 秒内退出，可手动结束。", pid);
    1
}

pub fn cmd_status() -> i32 {
    let pid = running_pid();
    let latest = fs::read_to_string(latest_file())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    println!(
        "运行状态：{}",
        pid.map(|p| format!("运行中 pid={}", p))
            .unwrap_or_else(|| "未运行".to_string())
    );
    println!("数据目录：{}", data_dir().display());
    println!(
        "最新会话：{}",
        if latest.is_empty() { "(无)" } else { &latest }
    );
    let latest_path = PathBuf::from(&latest);
    if !latest.is_empty() && latest_path.is_dir() {
        let hb = latest_path.join("heartbeat.txt");
        if let Ok(t) = fs::read_to_string(&hb) {
            println!("心跳：{}", t.trim());
        }
        for n in [
            "gpu.csv",
            "mem.csv",
            "events.csv",
            "wer.csv",
            "proc.csv",
            "alerts.log",
        ] {
            let p = latest_path.join(n);
            if let Ok(meta) = fs::metadata(&p) {
                println!("  {:12} {:9.1} KB", n, meta.len() as f64 / 1024.0);
            }
        }
        let g = latest_path.join("gpu.csv");
        if let Ok(raw) = fs::read_to_string(&g) {
            if let Some(last) = raw.lines().last() {
                let s: String = last.trim().chars().take(160).collect();
                println!("最后一条 GPU：{}", s);
            }
        }
    }
    let inc = incidents_dir();
    if inc.is_dir() {
        let n = fs::read_dir(&inc)
            .map(|rd| rd.flatten().filter(|e| e.path().is_dir()).count())
            .unwrap_or(0);
        if n > 0 {
            println!("事故目录：{} 个（{}）", n, inc.display());
        }
    }
    let _ = config_file();
    let _ = now_iso_sec();
    0
}

pub fn cmd_supervise() -> i32 {
    let _ = fs::create_dir_all(data_dir());
    let _ = fs::remove_file(stop_file());
    boot_log("[info] supervisor start");
    let Ok(exe) = std::env::current_exe() else {
        boot_log("[fatal] cannot resolve current_exe");
        return 1;
    };
    loop {
        if stop_file().exists() {
            boot_log("[info] STOP present, supervisor exit");
            return 0;
        }
        let mut child = match Command::new(&exe)
            .arg("run")
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                boot_log(&format!("[warn] spawn run failed: {}", e));
                thread::sleep(Duration::from_secs(5));
                continue;
            }
        };
        let rc = child
            .wait()
            .map(|s| s.code().unwrap_or(-1))
            .unwrap_or(-1);
        if stop_file().exists() {
            boot_log("[info] child stopped by STOP, supervisor exit");
            return 0;
        }
        boot_log(&format!("[warn] child exited rc={}, restart in 5s", rc));
        for _ in 0..10 {
            if stop_file().exists() {
                return 0;
            }
            thread::sleep(Duration::from_millis(500));
        }
    }
}

/// 无阻塞后台启动守护采集（供界面按钮直接调用，无需 .cmd）
pub fn start_supervisor_bg() -> Result<(), String> {
    if running_pid().is_some() {
        return Ok(());
    }
    let _ = fs::create_dir_all(data_dir());
    let _ = fs::remove_file(stop_file());
    let exe = std::env::current_exe().map_err(|e| format!("无法定位可执行文件: {}", e))?;
    Command::new(&exe)
        .arg("supervise")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("启动后台采集失败: {}", e))?;
    Ok(())
}

/// 无阻塞请求停止后台采集（供界面按钮直接调用）
pub fn request_stop_bg() {
    let _ = fs::create_dir_all(data_dir());
    let _ = File::create(stop_file());
    boot_log("[info] STOP requested from UI");
}

pub fn cmd_install_startup() -> i32 {
    match install_startup_reg() {
        Ok(_) => {
            println!("已通过注册表安装当前用户开机自启（HKCU\\...\\Run\\GPUCap）");
            0
        }
        Err(e) => {
            println!("安装开机自启失败：{}", e);
            1
        }
    }
}

pub fn cmd_uninstall_startup() -> i32 {
    match uninstall_startup_reg() {
        Ok(_) => {
            println!("已移除开机自启");
            0
        }
        Err(e) => {
            println!("移除失败：{}", e);
            1
        }
    }
}
