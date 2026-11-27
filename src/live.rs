//! 纯内存实时监测引擎：不创建会话目录、不写入任何磁盘文件，仅在内存中维护环形监测日志。

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Local;
use regex::Regex;
use serde::Serialize;

use crate::collectors::{EventTailer, GpuSampler, MemSampler, ProcSampler, WerWatcher};
use crate::cpu_probe::{CpuProbe, CpuSnapshot};
use crate::daemon::request_stop_bg;
use crate::infra::{
    check_and_update_driver_version, decode_throttle_label, detect_power_plan, load_config,
    running_pid, sessions_dir, EnvFingerprint,
};

pub const LIVE_SESSION_NAME: &str = "__live__";
const MAX_GPU_ROWS: usize = 300;
const MAX_CPU_POINTS: usize = 60;
const MAX_MEM_POINTS: usize = 120;
const MAX_EVENTS: usize = 300;
const MAX_WER: usize = 200;
const MAX_ALERTS: usize = 200;

#[derive(Clone, Serialize)]
pub struct LiveGpuRow {
    pub ts: String,
    pub temp: String,
    pub hotspot: String,
    pub power: String,
    pub core: String,
    pub memclk: String,
    pub pcie: String,
    pub util: String,
    pub vram: String,
    pub vram_total: String,
    pub vram_pct: f64,
    pub pcie_drop: bool,
    pub fan: String,
    pub thr: String,
    pub thr_label: String,
    pub ok: String,
    pub sev: &'static str,
}

#[derive(Clone, Serialize, Default)]
pub struct LiveMetricStats {
    pub min: f64,
    pub avg: f64,
    pub max: f64,
    pub count: usize,
}

#[derive(Clone, Serialize, Default)]
pub struct LiveStatsSummary {
    pub gpu_temp: Option<LiveMetricStats>,
    pub gpu_hotspot: Option<LiveMetricStats>,
    pub gpu_power: Option<LiveMetricStats>,
    pub gpu_core: Option<LiveMetricStats>,
    pub cpu_usage: Option<LiveMetricStats>,
    pub cpu_temp: Option<LiveMetricStats>,
}

pub fn calc_metric_stats<I: Iterator<Item = f64>>(iter: I) -> Option<LiveMetricStats> {
    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for v in iter {
        if v.is_finite() {
            if v < min_v {
                min_v = v;
            }
            if v > max_v {
                max_v = v;
            }
            sum += v;
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    let round1 = |x: f64| (x * 10.0).round() / 10.0;
    Some(LiveMetricStats {
        min: round1(min_v),
        avg: round1(sum / count as f64),
        max: round1(max_v),
        count,
    })
}

#[derive(Clone, Serialize)]
pub struct LiveCpuPoint {
    pub ts: String,
    pub total_usage: f32,
    pub avg_freq_mhz: u64,
    pub temp_c: Option<f64>,
    pub alert: bool,
}

#[derive(Clone, Serialize)]
pub struct LiveMemPoint {
    pub ts: String,
    pub commit_pct: f64,
    pub nonpaged_mb: u64,
}

#[derive(Clone, Serialize)]
pub struct LiveLogItem {
    pub time: String,
    pub level: String,
    pub source: String,
    pub message: String,
    pub raw: String,
    pub sev: &'static str,
}

#[derive(Clone)]
pub struct LiveSnapshot {
    pub running: bool,
    pub started_at: Option<String>,
    pub gpu_rows: Vec<LiveGpuRow>,
    pub cpu_points: Vec<LiveCpuPoint>,
    pub mem_points: Vec<LiveMemPoint>,
    pub events: Vec<LiveLogItem>,
    pub wer: Vec<LiveLogItem>,
    pub alerts: Vec<LiveLogItem>,
    pub gpu_summary: String,
    pub mem_summary: String,
    pub proc_summary: String,
    pub alert_count: usize,
    pub cpu: Option<CpuSnapshot>,
    pub stats: LiveStatsSummary,
    pub env: Option<EnvFingerprint>,
}

struct LiveBuffer {
    running: bool,
    has_data: bool,
    started_at: Option<String>,
    gpu_rows: VecDeque<LiveGpuRow>,
    cpu_points: VecDeque<LiveCpuPoint>,
    mem_points: VecDeque<LiveMemPoint>,
    events: VecDeque<LiveLogItem>,
    wer: VecDeque<LiveLogItem>,
    alerts: VecDeque<LiveLogItem>,
    gpu_summary: String,
    mem_summary: String,
    proc_summary: String,
    alert_count: usize,
    cpu: Option<CpuSnapshot>,
    env: Option<EnvFingerprint>,
}

impl Default for LiveBuffer {
    fn default() -> Self {
        Self {
            running: false,
            has_data: false,
            started_at: None,
            gpu_rows: VecDeque::new(),
            cpu_points: VecDeque::new(),
            mem_points: VecDeque::new(),
            events: VecDeque::new(),
            wer: VecDeque::new(),
            alerts: VecDeque::new(),
            gpu_summary: "—".to_string(),
            mem_summary: "—".to_string(),
            proc_summary: "—".to_string(),
            alert_count: 0,
            cpu: None,
            env: None,
        }
    }
}

struct LiveManager {
    buf: Mutex<LiveBuffer>,
    stop_flag: Mutex<Option<Arc<AtomicBool>>>,
}

static LIVE_MANAGER: OnceLock<LiveManager> = OnceLock::new();

fn manager() -> &'static LiveManager {
    LIVE_MANAGER.get_or_init(|| LiveManager {
        buf: Mutex::new(LiveBuffer::default()),
        stop_flag: Mutex::new(None),
    })
}

fn push_bounded<T>(q: &mut VecDeque<T>, item: T, cap: usize) {
    if q.len() >= cap {
        q.pop_front();
    }
    q.push_back(item);
}

pub fn is_running() -> bool {
    manager().buf.lock().map(|b| b.running).unwrap_or(false)
}

pub fn has_data() -> bool {
    manager().buf.lock().map(|b| b.has_data).unwrap_or(false)
}

pub fn snapshot() -> LiveSnapshot {
    let b = manager().buf.lock().unwrap_or_else(|e| e.into_inner());
    let gpu_rows: Vec<LiveGpuRow> = b.gpu_rows.iter().cloned().collect();
    let cpu_points: Vec<LiveCpuPoint> = b.cpu_points.iter().cloned().collect();
    let stats = LiveStatsSummary {
        gpu_temp: calc_metric_stats(gpu_rows.iter().filter_map(|r| r.temp.trim().parse::<f64>().ok())),
        gpu_hotspot: calc_metric_stats(gpu_rows.iter().filter_map(|r| r.hotspot.trim().parse::<f64>().ok())),
        gpu_power: calc_metric_stats(gpu_rows.iter().filter_map(|r| r.power.trim().parse::<f64>().ok())),
        gpu_core: calc_metric_stats(gpu_rows.iter().filter_map(|r| r.core.trim().parse::<f64>().ok())),
        cpu_usage: calc_metric_stats(cpu_points.iter().map(|p| f64::from(p.total_usage))),
        cpu_temp: calc_metric_stats(cpu_points.iter().filter_map(|p| p.temp_c)),
    };
    LiveSnapshot {
        running: b.running,
        started_at: b.started_at.clone(),
        gpu_rows,
        cpu_points,
        mem_points: b.mem_points.iter().cloned().collect(),
        events: b.events.iter().cloned().collect(),
        wer: b.wer.iter().cloned().collect(),
        alerts: b.alerts.iter().cloned().collect(),
        gpu_summary: b.gpu_summary.clone(),
        mem_summary: b.mem_summary.clone(),
        proc_summary: b.proc_summary.clone(),
        alert_count: b.alert_count,
        cpu: b.cpu.clone(),
        stats,
        env: b.env.clone(),
    }
}

/// 停止纯内存实时监测，保留已采集的最后一次内存日志供当前继续查看。
pub fn stop_live() {
    if let Ok(mut guard) = manager().stop_flag.lock() {
        if let Some(flag) = guard.take() {
            flag.store(true, Ordering::SeqCst);
        }
    }
    if let Ok(mut b) = manager().buf.lock() {
        b.running = false;
    }
}

/// 启动纯内存实时监测（与落盘采集互斥：若后台正在落盘采集则先停止）。
pub fn start_live() -> Result<(), String> {
    if running_pid().is_some() {
        request_stop_bg();
    }

    {
        let b = manager().buf.lock().unwrap_or_else(|e| e.into_inner());
        if b.running {
            return Ok(());
        }
    }

    // 清空上一轮内存缓冲并标记本轮启动
    {
        let mut b = manager().buf.lock().unwrap_or_else(|e| e.into_inner());
        b.running = true;
        b.has_data = true;
        b.started_at = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        b.gpu_rows.clear();
        b.cpu_points.clear();
        b.mem_points.clear();
        b.events.clear();
        b.wer.clear();
        b.alerts.clear();
        b.gpu_summary = "正在初始化显卡采集...".to_string();
        b.mem_summary = "—".to_string();
        b.proc_summary = "—".to_string();
        b.alert_count = 0;
        b.cpu = None;
        b.env = None;
    }

    let flag = Arc::new(AtomicBool::new(false));
    {
        let mut guard = manager().stop_flag.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(prev) = guard.replace(flag.clone()) {
            prev.store(true, Ordering::SeqCst);
        }
    }

    thread::Builder::new()
        .name("gpucap-live".into())
        .spawn(move || run_live_loop(flag))
        .map_err(|e| format!("启动实时监测线程失败: {e}"))?;

    Ok(())
}

fn run_live_loop(stop_flag: Arc<AtomicBool>) {
    let mut cfg = load_config();
    let gpu = GpuSampler::new(cfg.gpu_index);
    let mut cpu = CpuProbe::new();
    let mem = MemSampler;
    let mut proc = ProcSampler::new(&cfg.game_names);
    let mut events = EventTailer::new();
    let mut wer = WerWatcher::new();

    let (gpu_name, driver_version, vram_total_mb, pcie_max, vbios) = gpu.env_info();
    let os_version = sysinfo::System::long_os_version().unwrap_or_else(|_| "Windows".to_string());
    let power_plan = detect_power_plan();
    let driver_changed_from = check_and_update_driver_version(&driver_version);
    if let Ok(mut b) = manager().buf.lock() {
        if let Some(ref old_drv) = driver_changed_from {
            b.alert_count += 1;
            let msg = format!("检测到显卡驱动版本变更：{} -> {}", old_drv, driver_version);
            push_bounded(
                &mut b.alerts,
                LiveLogItem {
                    time: Local::now().format("%H:%M:%S").to_string(),
                    level: "INFO".to_string(),
                    source: "环境指纹".to_string(),
                    message: msg.clone(),
                    raw: msg,
                    sev: "warn",
                },
                MAX_ALERTS,
            );
        }
        b.env = Some(EnvFingerprint {
            gpu_name,
            driver_version,
            vram_total_mb,
            pcie_max,
            vbios,
            os_version,
            power_plan,
            driver_changed_from,
        });
    }

    let watch_re = Regex::new(
        r"(?i)WHEA-Logger|nvlddmkm|Display|Kernel-Power|BugCheck|Kernel-PnP|Disk|amdkmpfd|amdwddmg|LiveKernelEvent",
    )
    .expect("内置正则有效");
    let watch_ids: HashSet<i64> = [1, 17, 18, 19, 41, 1001, 4101, 6008, 14, 153]
        .into_iter()
        .collect();

    let mut next_gpu = Instant::now();
    let mut next_mem = Instant::now();
    let mut next_proc = Instant::now();
    let mut next_wer = Instant::now();
    let mut next_cfg = Instant::now() + Duration::from_secs(5);
    let mut last_alert_at: Option<Instant> = None;
    let mut last_cpu_alert_at: Option<Instant> = None;
    let mut last_vram_alert_at: Option<Instant> = None;
    let mut last_pcie_alert_at: Option<Instant> = None;

    while !stop_flag.load(Ordering::SeqCst) {
        let now = Instant::now();
        let ts = Local::now().format("%H:%M:%S").to_string();

        if now >= next_cfg {
            next_cfg = now + Duration::from_secs(5);
            cfg = load_config();
        }

        if now >= next_gpu {
            next_gpu = now + Duration::from_secs_f64(cfg.gpu_interval.max(0.5));
            let cpu_snap = cpu.sample();
            let cpu_temp_hit = cpu_snap.temp_c.is_some_and(|t| t >= cfg.cpu_temp_warn);
            let cpu_usage_hit = f64::from(cpu_snap.total_usage) >= cfg.cpu_usage_warn;
            let cpu_warn = cpu_temp_hit || cpu_usage_hit;
            let mut point_alert = cpu_warn;

            if let Ok(mut b) = manager().buf.lock() {
                if cpu_warn && last_cpu_alert_at.is_none_or(|t| t.elapsed() >= Duration::from_secs(30)) {
                    last_cpu_alert_at = Some(now);
                    b.alert_count += 1;
                    let temp_txt = cpu_snap
                        .temp_c
                        .map(|t| format!("{t:.1}°C"))
                        .unwrap_or_else(|| "—".to_string());
                    let msg = format!(
                        "CPU 实时指标达到预警阈值 (占用 {:.1}%, 温度 {}, 均频 {}MHz)",
                        cpu_snap.total_usage, temp_txt, cpu_snap.avg_freq_mhz
                    );
                    push_bounded(
                        &mut b.alerts,
                        LiveLogItem {
                            time: ts.clone(),
                            level: "WARN".to_string(),
                            source: "CPU 实时监测".to_string(),
                            message: msg.clone(),
                            raw: msg,
                            sev: "warn",
                        },
                        MAX_ALERTS,
                    );
                }
                b.cpu = Some(cpu_snap.clone());
            }
            let (res, _ms) = gpu.sample();
            match res {
                Ok(rec) => {
                    let temp = rec.get("temperature.gpu").cloned().unwrap_or_default();
                    let hotspot = rec.get("temperature.hotspot").cloned().unwrap_or_default();
                    let power = rec.get("power.draw").cloned().unwrap_or_default();
                    let core = rec.get("clocks.current.graphics").cloned().unwrap_or_default();
                    let memclk = rec.get("clocks.current.memory").cloned().unwrap_or_default();
                    let gen = rec.get("pcie.link.gen.current").cloned().unwrap_or_default();
                    let wid = rec.get("pcie.link.width.current").cloned().unwrap_or_default();
                    let util = rec.get("utilization.gpu").cloned().unwrap_or_default();
                    let vram = rec.get("memory.used").cloned().unwrap_or_default();
                    let vram_total = rec.get("memory.total").cloned().unwrap_or_default();
                    let gen_max_s = rec.get("pcie.link.gen.max").cloned().unwrap_or_default();
                    let wid_max_s = rec.get("pcie.link.width.max").cloned().unwrap_or_default();
                    let fan = rec.get("fan.speed").cloned().unwrap_or_default();
                    let thr_raw = rec.get("clocks_throttle_reasons.active").cloned().unwrap_or_default();

                    let pcie = if !gen.trim().is_empty() {
                        format!("Gen{}x{}", gen.trim(), wid.trim())
                    } else {
                        "—".to_string()
                    };
                    let thr_short = if thr_raw.trim().is_empty() {
                        String::new()
                    } else {
                        let hex = thr_raw.trim().trim_start_matches("0x").trim_start_matches("0X");
                        u64::from_str_radix(hex, 16)
                            .map(|v| format!("0x{:x}", v))
                            .unwrap_or_else(|_| thr_raw.chars().take(10).collect())
                    };
                    let thr_label = decode_throttle_label(&thr_short);

                    let temp_f = temp.trim().parse::<f64>().ok();
                    let hs_f = hotspot.trim().parse::<f64>().ok();
                    let power_f = power.trim().parse::<f64>().ok();
                    let util_f = util.trim().parse::<f64>().ok();
                    let gen_i = gen.trim().parse::<i64>().ok();
                    let wid_i = wid.trim().parse::<i64>().ok();
                    let gen_max_i = gen_max_s.trim().parse::<i64>().ok();
                    let wid_max_i = wid_max_s.trim().parse::<i64>().ok();
                    let vram_used_f = vram.trim().parse::<f64>().ok();
                    let vram_total_f = vram_total.trim().parse::<f64>().ok();
                    let vram_pct = match (vram_used_f, vram_total_f) {
                        (Some(u), Some(t)) if t > 0.0 => ((u / t) * 1000.0).round() / 10.0,
                        _ => 0.0,
                    };
                    let delta_f = match (temp_f, hs_f) {
                        (Some(t), Some(h)) => Some(h - t),
                        _ => None,
                    };

                    let high_load = power_f.is_some_and(|w| w >= 120.0) || util_f.is_some_and(|u| u >= 60.0);
                    let pcie_drop = cfg.pcie_drop_warn
                        && high_load
                        && (match (gen_i, gen_max_i) {
                            (Some(g), Some(gm)) => g > 0 && gm > 0 && g < gm,
                            _ => false,
                        } || match (wid_i, wid_max_i) {
                            (Some(w), Some(wm)) => w > 0 && wm > 0 && w < wm,
                            _ => false,
                        });
                    let vram_warn = vram_pct >= cfg.vram_warn_pct;

                    let warn = temp_f.is_some_and(|t| t >= cfg.temp_warn)
                        || hs_f.is_some_and(|h| h >= cfg.hotspot_warn)
                        || delta_f.is_some_and(|d| d >= cfg.hotspot_delta_warn)
                        || power_f.is_some_and(|w| w >= cfg.power_warn)
                        || vram_warn
                        || pcie_drop;

                    let thr_active = !thr_short.is_empty() && thr_short != "0x0" && thr_short != "0x1";
                    if warn || thr_active {
                        point_alert = true;
                    }

                    let sev = if warn { "warn" } else { "normal" };
                    let summary = format!(
                        "{}°C  {}W  {}MHz  {}  util {}%  VRAM {}MB",
                        temp, power, core, pcie, util, vram
                    );

                    if let Ok(mut b) = manager().buf.lock() {
                        b.gpu_summary = summary;
                        push_bounded(
                            &mut b.gpu_rows,
                            LiveGpuRow {
                                ts: ts.clone(),
                                temp,
                                hotspot,
                                power,
                                core,
                                memclk,
                                pcie: pcie.clone(),
                                util,
                                vram: vram.clone(),
                                vram_total: vram_total.clone(),
                                vram_pct,
                                pcie_drop,
                                fan,
                                thr: thr_short,
                                thr_label,
                                ok: "OK".to_string(),
                                sev,
                            },
                            MAX_GPU_ROWS,
                        );
                        push_bounded(
                            &mut b.cpu_points,
                            LiveCpuPoint {
                                ts: ts.clone(),
                                total_usage: cpu_snap.total_usage,
                                avg_freq_mhz: cpu_snap.avg_freq_mhz,
                                temp_c: cpu_snap.temp_c,
                                alert: point_alert,
                            },
                            MAX_CPU_POINTS,
                        );
                        if warn && last_alert_at.is_none_or(|t| t.elapsed() >= Duration::from_secs(10)) {
                            last_alert_at = Some(now);
                            b.alert_count += 1;
                            let msg = format!("GPU 实时指标达到预警阈值 ({})", b.gpu_summary);
                            push_bounded(
                                &mut b.alerts,
                                LiveLogItem {
                                    time: ts.clone(),
                                    level: "WARN".to_string(),
                                    source: "GPU 实时监测".to_string(),
                                    message: msg.clone(),
                                    raw: msg,
                                    sev: "warn",
                                },
                                MAX_ALERTS,
                            );
                        }
                        if vram_warn && last_vram_alert_at.is_none_or(|t| t.elapsed() >= Duration::from_secs(30)) {
                            last_vram_alert_at = Some(now);
                            b.alert_count += 1;
                            let msg = format!(
                                "显存接近满载：已用 {} / {} MB ({:.1}%)",
                                vram, vram_total, vram_pct
                            );
                            push_bounded(
                                &mut b.alerts,
                                LiveLogItem {
                                    time: ts.clone(),
                                    level: "WARN".to_string(),
                                    source: "显存专项预警".to_string(),
                                    message: msg.clone(),
                                    raw: msg,
                                    sev: "warn",
                                },
                                MAX_ALERTS,
                            );
                        }
                        if pcie_drop && last_pcie_alert_at.is_none_or(|t| t.elapsed() >= Duration::from_secs(30)) {
                            last_pcie_alert_at = Some(now);
                            b.alert_count += 1;
                            let msg = format!(
                                "高负载下 PCIe 带宽降级：当前 {}（硬件上限 Gen{}x{}）",
                                pcie, gen_max_s, wid_max_s
                            );
                            push_bounded(
                                &mut b.alerts,
                                LiveLogItem {
                                    time: ts.clone(),
                                    level: "WARN".to_string(),
                                    source: "PCIe 链路预警".to_string(),
                                    message: msg.clone(),
                                    raw: msg,
                                    sev: "warn",
                                },
                                MAX_ALERTS,
                            );
                        }
                    }
                }
                Err(err) => {
                    if let Ok(mut b) = manager().buf.lock() {
                        b.gpu_summary = format!("采样异常: {err}");
                        push_bounded(
                            &mut b.cpu_points,
                            LiveCpuPoint {
                                ts: ts.clone(),
                                total_usage: cpu_snap.total_usage,
                                avg_freq_mhz: cpu_snap.avg_freq_mhz,
                                temp_c: cpu_snap.temp_c,
                                alert: true,
                            },
                            MAX_CPU_POINTS,
                        );
                        push_bounded(
                            &mut b.gpu_rows,
                            LiveGpuRow {
                                ts: ts.clone(),
                                temp: "—".to_string(),
                                hotspot: String::new(),
                                power: "—".to_string(),
                                core: "—".to_string(),
                                memclk: "—".to_string(),
                                pcie: "—".to_string(),
                                util: "—".to_string(),
                                vram: "—".to_string(),
                                vram_total: String::new(),
                                vram_pct: 0.0,
                                pcie_drop: false,
                                fan: "—".to_string(),
                                thr: String::new(),
                                thr_label: String::new(),
                                ok: "ERR".to_string(),
                                sev: "bad",
                            },
                            MAX_GPU_ROWS,
                        );
                    }
                }
            }
        }

        if now >= next_mem {
            next_mem = now + Duration::from_secs_f64(cfg.mem_interval.max(1.0));
            if let Some(m) = mem.sample() {
                let pct = if m.commit_limit_gb > 0.0 {
                    (m.commit_gb / m.commit_limit_gb) * 100.0
                } else {
                    0.0
                };
                let summary = format!(
                    "内存 {:.1}/{:.1}GB  可用 {}MB  非分页 {}MB",
                    m.commit_gb, m.commit_limit_gb, m.phys_avail_mb, m.kernel_nonpaged_mb
                );
                if let Ok(mut b) = manager().buf.lock() {
                    b.mem_summary = summary;
                    push_bounded(
                        &mut b.mem_points,
                        LiveMemPoint {
                            ts: ts.clone(),
                            commit_pct: (pct * 10.0).round() / 10.0,
                            nonpaged_mb: m.kernel_nonpaged_mb,
                        },
                        MAX_MEM_POINTS,
                    );
                }
            }
        }

        if now >= next_proc {
            next_proc = now + Duration::from_secs_f64(cfg.proc_interval.max(1.0));
            let infos = proc.sample();
            let summary = if let Some(first) = infos.first() {
                format!("{} (PID {})", first.name, first.pid)
            } else {
                "未运行".to_string()
            };
            if let Ok(mut b) = manager().buf.lock() {
                b.proc_summary = summary;
            }
        }

        let ev_list = events.poll(Duration::from_millis(80));
        if !ev_list.is_empty() {
            if let Ok(mut b) = manager().buf.lock() {
                for ev in ev_list {
                    let level = ev.get("level").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let source = ev.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let msg = ev.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let eid = ev.get("event_id").and_then(|v| v.as_i64()).unwrap_or(0);
                    let t_str = ev
                        .get("time")
                        .and_then(|v| v.as_str())
                        .map(|s| if s.len() > 19 { s[11..19].to_string() } else { s.to_string() })
                        .unwrap_or_else(|| ts.clone());
                    let is_bad = level.contains("错误") || level.contains("Error") || level.contains("严重");
                    let is_watch = watch_re.is_match(&source) || watch_re.is_match(&msg) || watch_ids.contains(&eid);
                    let sev = if is_bad {
                        "bad"
                    } else if is_watch || level.contains("警告") || level.contains("Warning") {
                        "warn"
                    } else {
                        "normal"
                    };
                    let raw = format!("[{t_str}] [{level}] {source} (#{eid}) {msg}");
                    let item = LiveLogItem {
                        time: t_str,
                        level: level.clone(),
                        source: source.clone(),
                        message: msg.clone(),
                        raw,
                        sev,
                    };
                    push_bounded(&mut b.events, item.clone(), MAX_EVENTS);
                    if sev != "normal" && is_watch {
                        b.alert_count += 1;
                        push_bounded(&mut b.alerts, item, MAX_ALERTS);
                    }
                }
            }
        }

        if now >= next_wer {
            next_wer = now + Duration::from_secs_f64(cfg.wer_interval.max(2.0));
            let w_list = wer.poll();
            if !w_list.is_empty() {
                if let Ok(mut b) = manager().buf.lock() {
                    for w in w_list {
                        let raw = format!("{} | {} | {} | {}", w.created, w.kind, w.name, w.path);
                        let item = LiveLogItem {
                            time: w.created,
                            level: w.kind,
                            source: w.path,
                            message: w.name,
                            raw,
                            sev: "bad",
                        };
                        b.alert_count += 1;
                        push_bounded(&mut b.wer, item.clone(), MAX_WER);
                        push_bounded(&mut b.alerts, item, MAX_ALERTS);
                    }
                }
            }
        }

        thread::sleep(Duration::from_millis(200));
    }

    if let Ok(mut b) = manager().buf.lock() {
        b.running = false;
    }
}

/// 基于当前内存实时快照生成 Markdown 诊断摘要。
pub fn format_diagnostic_summary(snap: &LiveSnapshot) -> String {
    let start = snap.started_at.as_deref().unwrap_or("—");
    let max_gpu_temp = snap
        .gpu_rows
        .iter()
        .filter_map(|r| r.temp.trim().parse::<f64>().ok())
        .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |a| a.max(v))));
    let max_hotspot = snap
        .gpu_rows
        .iter()
        .filter_map(|r| r.hotspot.trim().parse::<f64>().ok())
        .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |a| a.max(v))));
    let max_power = snap
        .gpu_rows
        .iter()
        .filter_map(|r| r.power.trim().parse::<f64>().ok())
        .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |a| a.max(v))));
    let throttle_count = snap
        .gpu_rows
        .iter()
        .filter(|r| !r.thr.is_empty() && r.thr != "0x0" && r.thr != "0x1")
        .count();

    let max_cpu_usage = snap
        .cpu_points
        .iter()
        .map(|p| p.total_usage)
        .fold(0.0f32, f32::max);
    let max_cpu_temp = snap
        .cpu_points
        .iter()
        .filter_map(|p| p.temp_c)
        .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |a| a.max(v))));

    let mut out = String::new();
    out.push_str("# GPUCap 实时监测诊断摘要\n\n");
    if let Some(ref env) = snap.env {
        out.push_str("## 系统环境指纹\n");
        out.push_str(&format!("- 显卡：{} (VBIOS {}, 显存 {} MB, 上限 {})\n", env.gpu_name, env.vbios, env.vram_total_mb, env.pcie_max));
        out.push_str(&format!("- 驱动版本：{}\n", env.driver_version));
        if let Some(ref old) = env.driver_changed_from {
            out.push_str(&format!("- 驱动变更记录：上次 {} -> 本次 {}\n", old, env.driver_version));
        }
        out.push_str(&format!("- 操作系统：{} · 电源计划：{}\n\n", env.os_version, env.power_plan));
    }
    out.push_str(&format!("- 启动时间：{start}\n"));
    out.push_str(&format!(
        "- 监测状态：{}\n",
        if snap.running { "实时监测中" } else { "已停止（内存快照）" }
    ));
    out.push_str(&format!("- 采样规模：GPU {} 条 / CPU趋势 {} 点 / 系统事件 {} 条 / 崩溃 {} 条\n",
        snap.gpu_rows.len(),
        snap.cpu_points.len(),
        snap.events.len(),
        snap.wer.len()
    ));
    out.push_str(&format!(
        "- GPU 峰值：温度 {} / 热点 {} / 功耗 {} / 降频事件 {} 次\n",
        max_gpu_temp.map(|v| format!("{v:.1}°C")).unwrap_or_else(|| "—".into()),
        max_hotspot.map(|v| format!("{v:.1}°C")).unwrap_or_else(|| "—".into()),
        max_power.map(|v| format!("{v:.1}W")).unwrap_or_else(|| "—".into()),
        throttle_count
    ));
    out.push_str(&format!(
        "- CPU 峰值：占用 {:.1}% / 温度 {}\n",
        max_cpu_usage,
        max_cpu_temp.map(|v| format!("{v:.1}°C")).unwrap_or_else(|| "—".into())
    ));
    if let Some(ref cpu) = snap.cpu {
        if let Some(top) = cpu.top_cpu_procs.first() {
            out.push_str(&format!(
                "- 当前最高 CPU 进程：{} (PID {}, {:.1}%, {} MB)\n",
                top.name, top.pid, top.cpu_pct, top.mem_mb
            ));
        }
    }
    out.push_str(&format!("- 告警总数：{}\n", snap.alert_count));
    if !snap.alerts.is_empty() {
        out.push_str("\n## 最近告警记录\n");
        for item in snap.alerts.iter().rev().take(5) {
            out.push_str(&format!("- [{}] [{}] {}: {}\n", item.time, item.level, item.source, item.message));
        }
    }
    out
}

pub fn build_diagnostic_summary() -> String {
    let snap = snapshot();
    format_diagnostic_summary(&snap)
}

/// 仅在用户主动触发时，将当前内存实时日志补存为正式会话目录。
pub fn save_snapshot_to_session() -> Result<String, String> {
    let snap = snapshot();
    if snap.gpu_rows.is_empty()
        && snap.cpu_points.is_empty()
        && snap.events.is_empty()
        && snap.wer.is_empty()
        && snap.alerts.is_empty()
    {
        return Err("当前内存中没有可保存的实时监测数据".to_string());
    }

    let sess_name = format!("live_{}", Local::now().format("%Y%m%d_%H%M%S"));
    let dir = sessions_dir().join(&sess_name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建会话目录失败: {e}"))?;

    // 写入 GPU CSV
    let mut gpu_csv = String::from("ts,temperature.gpu,temperature.hotspot,power.draw,clocks.current.graphics,clocks.current.memory,pcie,utilization.gpu,memory.used,fan.speed,clocks_throttle_reasons.active,ok\n");
    for r in &snap.gpu_rows {
        gpu_csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            r.ts, r.temp, r.hotspot, r.power, r.core, r.memclk, r.pcie, r.util, r.vram, r.fan, r.thr, r.ok
        ));
    }
    std::fs::write(dir.join("gpu.csv"), gpu_csv).map_err(|e| format!("写入 gpu.csv 失败: {e}"))?;

    // 写入内存采样 CSV（兼容 web 端会话解析格式）
    let mut mem_csv = String::from("ts,commit_gb,limit_gb,phys_avail_mb,pagefile_mb,pool_paged_mb,pool_nonpaged_mb,kernel_nonpaged_mb\n");
    for m in &snap.mem_points {
        mem_csv.push_str(&format!("{},{},100,0,0,0,0,{}\n", m.ts, m.commit_pct, m.nonpaged_mb));
    }
    std::fs::write(dir.join("mem.csv"), mem_csv).map_err(|e| format!("写入 mem.csv 失败: {e}"))?;

    let ev_txt: String = snap.events.iter().map(|e| format!("{}\n", e.raw)).collect();
    std::fs::write(dir.join("events.log"), ev_txt).map_err(|e| format!("写入 events.log 失败: {e}"))?;

    let wer_txt: String = snap.wer.iter().map(|w| format!("{}\n", w.raw)).collect();
    std::fs::write(dir.join("wer.log"), wer_txt).map_err(|e| format!("写入 wer.log 失败: {e}"))?;

    let alert_txt: String = snap
        .alerts
        .iter()
        .map(|a| format!("{} [{}] {}\n", a.time, a.level, a.message))
        .collect();
    std::fs::write(dir.join("alerts.log"), alert_txt).map_err(|e| format!("写入 alerts.log 失败: {e}"))?;

    let summary_md = format_diagnostic_summary(&snap);
    std::fs::write(dir.join("summary.md"), summary_md).map_err(|e| format!("写入 summary.md 失败: {e}"))?;

    Ok(sess_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_bounded_and_summary() {
        let mut q = VecDeque::new();
        for i in 0..5 {
            push_bounded(&mut q, i, 3);
        }
        assert_eq!(q.into_iter().collect::<Vec<_>>(), vec![2, 3, 4]);

        let snap = LiveSnapshot {
            running: false,
            started_at: Some("2026-11-25 22:30:00".into()),
            gpu_rows: vec![LiveGpuRow {
                ts: "22:30:01".into(),
                temp: "72.0".into(),
                hotspot: "84.0".into(),
                power: "260.0".into(),
                core: "2400".into(),
                memclk: "10500".into(),
                pcie: "Gen4x16".into(),
                util: "98".into(),
                vram: "8192".into(),
                vram_total: "16384".into(),
                vram_pct: 50.0,
                pcie_drop: false,
                fan: "60".into(),
                thr: "0x0".into(),
                thr_label: String::new(),
                ok: "OK".into(),
                sev: "normal",
            }],
            cpu_points: vec![LiveCpuPoint {
                ts: "22:30:01".into(),
                total_usage: 45.5,
                avg_freq_mhz: 4200,
                temp_c: Some(68.0),
                alert: false,
            }],
            mem_points: vec![],
            events: vec![],
            wer: vec![],
            alerts: vec![],
            gpu_summary: "72°C".into(),
            mem_summary: "—".into(),
            proc_summary: "—".into(),
            alert_count: 0,
            cpu: None,
            stats: LiveStatsSummary::default(),
            env: None,
        };
        let md = format_diagnostic_summary(&snap);
        assert!(md.contains("72.0°C"));
        assert!(md.contains("45.5%"));
        assert!(md.contains("68.0°C"));
        assert_eq!(decode_throttle_label("0x4"), "功耗墙");
        assert_eq!(decode_throttle_label("0xc"), "功耗墙+温度墙");
    }
}
