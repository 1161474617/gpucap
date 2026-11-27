//! 基于 eframe/egui 的原生实时监控界面：
//! 包含实时 GPU、双列趋势曲线（含内存/内核池与告警线）、系统事件、WER、告警与事故取证直览、
//! 一键打包导出、可视化参数设置及系统托盘常驻。

use crate::collectors::GpuSampler;
use crate::daemon::{request_stop_bg, start_supervisor_bg};
use crate::infra::{
    data_dir, delete_session_data, export_session_zip, incidents_dir, install_startup_reg,
    is_startup_installed, latest_file, latest_session_path, load_config, open_folder_in_explorer,
    read_startup_reg_cmd, running_pid, save_config, sessions_dir, set_main_window_visible,
    uninstall_startup_reg, Config, UiSingleton,
};
use crate::tray::{TrayAction, TrayManager};
use crate::web::{start_web_server, WebServerHandle, WEB_PORT};
use chrono::{DateTime, Local};
use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, RichText, Stroke};
use egui_extras::{Column, TableBuilder};
use egui_plot::{Line, LineStyle, Plot, PlotPoint, PlotPoints, Points, Text, VLine};
use std::collections::{HashSet, VecDeque};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const COL_BG: Color32 = Color32::from_rgb(0x11, 0x15, 0x1c);
const COL_PANEL: Color32 = Color32::from_rgb(0x16, 0x1b, 0x24);
const COL_BTN: Color32 = Color32::from_rgb(0x22, 0x30, 0x3f);
const COL_HEAD: Color32 = Color32::from_rgb(0x1d, 0x27, 0x35);
const COL_FG: Color32 = Color32::from_rgb(0xd6, 0xdd, 0xe8);
const COL_DIM: Color32 = Color32::from_rgb(0x7f, 0x8b, 0x9c);
const COL_WARN: Color32 = Color32::from_rgb(0xff, 0xb0, 0x20);
const COL_BAD: Color32 = Color32::from_rgb(0xff, 0x4d, 0x4f);
const COL_OK: Color32 = Color32::from_rgb(0x3d, 0xdc, 0x84);
const COL_ACCENT: Color32 = Color32::from_rgb(0x4d, 0xa3, 0xff);
const COL_BORDER: Color32 = Color32::from_rgb(0x26, 0x31, 0x42);

/// 首次打开大文件时默认从末尾回溯 128KB（兼顾秒开速度）；点击“加载全部”时从头读取
const INITIAL_TAIL_BYTES: u64 = 128 * 1024;
const MAX_GPU_ROWS_NORMAL: usize = 600;
const MAX_GPU_ROWS_FULL: usize = 20000;
const MAX_LOG_LINES: usize = 2000;

/// 低内存增量文件尾部读取器
struct Tail {
    path: PathBuf,
    offset: u64,
    full_mode: bool,
}

impl Tail {
    fn new(path: PathBuf, full_mode: bool) -> Self {
        Self {
            path,
            offset: 0,
            full_mode,
        }
    }

    fn read_new_with<F>(&mut self, mut on_line: F)
    where
        F: FnMut(&str),
    {
        let Ok(meta) = fs::metadata(&self.path) else {
            return;
        };
        let len = meta.len();
        if len < self.offset {
            self.offset = 0;
        }
        if len == self.offset {
            return;
        }
        let Ok(mut f) = File::open(&self.path) else {
            return;
        };

        let mut skip_partial_first = false;
        if !self.full_mode && self.offset == 0 && len > INITIAL_TAIL_BYTES {
            self.offset = len - INITIAL_TAIL_BYTES;
            skip_partial_first = true;
        }

        if f.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }

        let mut reader = BufReader::with_capacity(16 * 1024, f);
        let mut buf = String::with_capacity(256);
        let mut is_first = true;
        loop {
            buf.clear();
            match reader.read_line(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    // 采集器可能正写到半行：不消费不完整的行，等下一轮再读
                    if !buf.ends_with('\n') {
                        break;
                    }
                    self.offset += n as u64;
                    if is_first && skip_partial_first {
                        is_first = false;
                        continue;
                    }
                    is_first = false;
                    let trimmed = buf.trim_end_matches(['\r', '\n']);
                    if !trimmed.trim().is_empty() {
                        on_line(trimmed);
                    }
                }
                Err(_) => break,
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Gpu,
    Cpu,
    Trend,
    Events,
    Wer,
    Alerts,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Severity {
    Normal,
    Warn,
    Bad,
}

impl Severity {
    fn color(self) -> Color32 {
        match self {
            Severity::Normal => COL_FG,
            Severity::Warn => COL_WARN,
            Severity::Bad => COL_BAD,
        }
    }
}

struct GpuRow {
    ts: String,
    temp: String,
    hotspot: String,
    hotspot_delta: String,
    power: String,
    core: String,
    memclk: String,
    pcie: String,
    util: String,
    vram: String,
    fan: String,
    thr_label: String,
    ok: String,
    sev: Severity,
}

struct MemPoint {
    ts: String,
    commit_pct: f64,
    nonpaged_mb: f64,
}

struct LogEntry {
    time: String,
    level: String,
    source: String,
    message: String,
    raw: String,
    sev: Severity,
}

impl LogEntry {
    fn matches(&self, filter_lower: &str) -> bool {
        if filter_lower.is_empty() {
            return true;
        }
        self.time.to_lowercase().contains(filter_lower)
            || self.level.to_lowercase().contains(filter_lower)
            || self.source.to_lowercase().contains(filter_lower)
            || self.message.to_lowercase().contains(filter_lower)
    }
}

struct HeartbeatState {
    text: String,
    color: Color32,
}

/// 事故取证目录条目
struct IncidentEntry {
    name: String,
    dir: PathBuf,
    belongs_to_session: bool,
    reason: String,
}

/// 参数设置弹窗编辑缓冲
struct SettingsDraft {
    cfg: Config,
    game_names_text: String,
    gpus: Vec<(usize, String)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UiLang {
    Zh,
    En,
}

impl UiLang {
    fn from_cfg(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("en") {
            Self::En
        } else {
            Self::Zh
        }
    }
}

fn tr(lang: UiLang, zh: &'static str, en: &'static str) -> &'static str {
    if lang == UiLang::En {
        en
    } else {
        zh
    }
}

fn tr_thr(lang: UiLang, s: &str) -> String {
    if lang != UiLang::En || s.is_empty() || s == "—" {
        return s.to_string();
    }
    s.replace("功耗墙", "PowerCap")
        .replace("温度墙", "ThermalCap")
        .replace("软件温控", "SWThermal")
        .replace("硬件降频", "HWSlowdown")
        .replace("同步限频", "SyncBoost")
        .replace("显示限制", "DisplayCap")
        .replace("应用限频", "AppClock")
        .replace("空闲", "Idle")
}

pub struct GpuCapApp {
    lang: UiLang,
    show_lang_picker: bool,
    session: Option<PathBuf>,
    sessions: Vec<PathBuf>,
    selected_idx: usize,
    paused: bool,
    only_bad: bool,
    full_history: bool,
    active_tab: Tab,
    tail_gpu: Option<Tail>,
    tail_mem: Option<Tail>,
    tail_proc: Option<Tail>,
    tail_events: Option<Tail>,
    tail_wer: Option<Tail>,
    tail_alerts: Option<Tail>,
    gpu_rows: VecDeque<GpuRow>,
    mem_points: VecDeque<MemPoint>,
    last_gpu_summary: String,
    last_mem_summary: String,
    last_proc_summary: String,
    event_lines: VecDeque<LogEntry>,
    wer_lines: VecDeque<LogEntry>,
    alert_lines: VecDeque<LogEntry>,
    log_filter: String,
    trend_window: usize,
    alert_count: usize,
    heartbeat: HeartbeatState,
    collector_running: bool,
    live_running: bool,
    live_viewing: bool,
    live_cpu: Option<crate::cpu_probe::CpuSnapshot>,
    live_cpu_points: Vec<crate::live::LiveCpuPoint>,
    live_stats: crate::live::LiveStatsSummary,
    live_env: Option<crate::infra::EnvFingerprint>,
    hud_mode: bool,
    cpu_cores_expanded: bool,
    last_alert_notified: usize,
    live_banner_msg: Option<(String, Instant)>,
    live_action_msg: Option<(String, Color32, Instant)>,
    startup_enabled: bool,
    minimize_to_tray: bool,
    awaiting_new_session: bool,
    stopping_until: Option<Instant>,
    starting_until: Option<Instant>,
    hb_fail: u32,
    status_hint: Option<(String, Color32, Instant)>,
    last_tick: Instant,
    title_dirty: bool,
    copy_notice_until: Option<Instant>,
    pending_delete: Option<PathBuf>,
    batch_delete_open: bool,
    batch_selected: HashSet<PathBuf>,
    settings_draft: Option<SettingsDraft>,
    incidents_cache: Vec<IncidentEntry>,
    selected_incident: usize,
    incident_preview_file: String,
    incident_preview_text: String,
    last_incidents_scan: Instant,
    tray: Option<TrayManager>,
    _singleton: Arc<UiSingleton>,
    window_visible: bool,
    allow_quit: bool,
    scroll_gpu_bottom: bool,
    init_loaded: bool,
    web_server: Option<WebServerHandle>,
    web_port: u16,
}

impl GpuCapApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        session: Option<PathBuf>,
        singleton: Arc<UiSingleton>,
        preloaded_fonts: Option<FontDefinitions>,
        minimized: bool,
    ) -> Self {
        if let Some(fonts) = preloaded_fonts {
            cc.egui_ctx.set_fonts(fonts);
        } else {
            Self::setup_fonts(&cc.egui_ctx);
        }
        Self::setup_theme(&cc.egui_ctx);

        let cfg = load_config();
        let sessions = list_sessions();
        let selected_idx = session
            .as_ref()
            .and_then(|s| sessions.iter().position(|p| p == s))
            .unwrap_or(0);
        let tray = TrayManager::new(cc.egui_ctx.clone(), request_stop_bg).ok();

        // 若已开启自启但注册表仍是旧版命令（或路径变更），自动平滑更新为当前可执行文件托盘自启
        let startup_enabled = is_startup_installed();
        if startup_enabled {
            if let Ok(exe) = std::env::current_exe() {
                let expected = format!("\"{}\" ui --minimized", exe.display());
                if read_startup_reg_cmd().as_deref() != Some(expected.as_str()) {
                    let _ = install_startup_reg();
                }
            }
        }
        if minimized {
            set_main_window_visible(false);
            let _ = start_supervisor_bg();
            crate::infra::boot_log("[info] started minimized in tray via autostart");
        }

        // 后台线程阻塞等待二次启动的唤醒信号，收到后立刻恢复并置顶主窗口
        let sig = Arc::clone(&singleton);
        let wake_ctx = cc.egui_ctx.clone();
        thread::spawn(move || {
            while sig.wait_wakeup() {
                set_main_window_visible(true);
                wake_ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                wake_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                wake_ctx.request_repaint();
            }
        });

        let lang = UiLang::from_cfg(&cfg.language);
        let show_lang_picker = cfg.language.trim().is_empty();

        Self {
            lang,
            show_lang_picker,
            tail_gpu: session
                .as_ref()
                .map(|s| Tail::new(s.join("gpu.csv"), false)),
            tail_mem: session
                .as_ref()
                .map(|s| Tail::new(s.join("mem.csv"), false)),
            tail_proc: session
                .as_ref()
                .map(|s| Tail::new(s.join("proc.csv"), false)),
            tail_events: session
                .as_ref()
                .map(|s| Tail::new(s.join("events.csv"), false)),
            tail_wer: session
                .as_ref()
                .map(|s| Tail::new(s.join("wer.csv"), false)),
            tail_alerts: session
                .as_ref()
                .map(|s| Tail::new(s.join("alerts.log"), false)),
            session,
            sessions,
            selected_idx,
            paused: false,
            only_bad: false,
            full_history: false,
            active_tab: Tab::Gpu,
            gpu_rows: VecDeque::with_capacity(MAX_GPU_ROWS_NORMAL + 16),
            mem_points: VecDeque::with_capacity(256),
            last_gpu_summary: "—".to_string(),
            last_mem_summary: String::new(),
            last_proc_summary: String::new(),
            event_lines: VecDeque::with_capacity(256),
            wer_lines: VecDeque::with_capacity(64),
            alert_lines: VecDeque::with_capacity(128),
            log_filter: String::new(),
            trend_window: 20,
            alert_count: 0,
            heartbeat: HeartbeatState {
                text: "● 等待数据".to_string(),
                color: COL_DIM,
            },
            collector_running: running_pid().is_some() || minimized,
            live_running: false,
            live_viewing: false,
            live_cpu: None,
            live_cpu_points: Vec::new(),
            live_stats: crate::live::LiveStatsSummary::default(),
            live_env: None,
            hud_mode: false,
            cpu_cores_expanded: true,
            last_alert_notified: 0,
            live_banner_msg: None,
            live_action_msg: None,
            startup_enabled,
            minimize_to_tray: cfg.minimize_to_tray,
            awaiting_new_session: false,
            stopping_until: None,
            starting_until: None,
            hb_fail: 0,
            status_hint: None,
            last_tick: Instant::now() - Duration::from_secs(2),
            title_dirty: false,
            copy_notice_until: None,
            pending_delete: None,
            batch_delete_open: false,
            batch_selected: HashSet::new(),
            settings_draft: None,
            incidents_cache: Vec::new(),
            selected_incident: 0,
            incident_preview_file: "incident.json".to_string(),
            incident_preview_text: String::new(),
            last_incidents_scan: Instant::now() - Duration::from_secs(10),
            tray,
            _singleton: singleton,
            window_visible: !minimized,
            allow_quit: false,
            scroll_gpu_bottom: false,
            init_loaded: false,
            web_server: None,
            web_port: if cfg.web_port == 0 { WEB_PORT } else { cfg.web_port },
        }
    }

    /// 预读系统字体（可在后台线程与窗口初始化并行执行）
    fn build_fonts() -> Option<FontDefinitions> {
        let mut fonts = FontDefinitions::default();
        let msyh_candidates = [
            r"C:\Windows\Fonts\msyh.ttc",
            r"C:\Windows\Fonts\msyh.ttf",
            r"C:\Windows\Fonts\simhei.ttf",
        ];
        let mut loaded_cn = false;
        for path in msyh_candidates {
            if let Ok(bytes) = fs::read(path) {
                fonts
                    .font_data
                    .insert("msyh".to_string(), FontData::from_owned(bytes));
                if let Some(fam) = fonts.families.get_mut(&FontFamily::Proportional) {
                    fam.insert(0, "msyh".to_string());
                }
                if let Some(fam) = fonts.families.get_mut(&FontFamily::Monospace) {
                    fam.push("msyh".to_string());
                }
                loaded_cn = true;
                break;
            }
        }
        if let Ok(bytes) = fs::read(r"C:\Windows\Fonts\consola.ttf") {
            fonts
                .font_data
                .insert("consola".to_string(), FontData::from_owned(bytes));
            if let Some(fam) = fonts.families.get_mut(&FontFamily::Monospace) {
                fam.insert(0, "consola".to_string());
            }
        }
        if loaded_cn {
            Some(fonts)
        } else {
            None
        }
    }

    /// 加载系统自带微软雅黑字体与等宽字体
    fn setup_fonts(ctx: &egui::Context) {
        if let Some(fonts) = Self::build_fonts() {
            ctx.set_fonts(fonts);
        }
    }

    /// 配置暗黑主题配色与圆角间距规范
    fn setup_theme(ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.spacing.interact_size.y = 28.0;
        let r = egui::Rounding::same(5.0);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = COL_BG;
        visuals.window_fill = COL_PANEL;
        visuals.extreme_bg_color = Color32::from_rgb(0x0d, 0x11, 0x17);
        visuals.window_rounding = egui::Rounding::same(8.0);
        visuals.window_stroke = Stroke::new(1.0_f32, COL_BORDER);
        visuals.widgets.noninteractive.bg_fill = COL_PANEL;
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, COL_FG);
        visuals.widgets.noninteractive.rounding = r;
        visuals.widgets.inactive.bg_fill = COL_BTN;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, COL_FG);
        visuals.widgets.inactive.rounding = r;
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x2d, 0x3e, 0x52);
        visuals.widgets.hovered.rounding = r;
        visuals.widgets.active.bg_fill = Color32::from_rgb(0x36, 0x4b, 0x63);
        visuals.widgets.active.rounding = r;
        visuals.selection.bg_fill = Color32::from_rgb(0x26, 0x4f, 0x78);
        style.visuals = visuals;
        ctx.set_style(style);
    }

    fn switch_session(&mut self, path: PathBuf) {
        self.live_viewing = false;
        self.full_history = false;
        self.log_filter.clear();
        self.session = Some(path.clone());
        self.reset_tails(false);
        self.refresh_incidents();
        self.tick();
    }

    fn reset_tails(&mut self, full: bool) {
        self.full_history = full;
        if let Some(ref path) = self.session {
            self.tail_gpu = Some(Tail::new(path.join("gpu.csv"), full));
            self.tail_mem = Some(Tail::new(path.join("mem.csv"), full));
            self.tail_proc = Some(Tail::new(path.join("proc.csv"), full));
            self.tail_events = Some(Tail::new(path.join("events.csv"), full));
            self.tail_wer = Some(Tail::new(path.join("wer.csv"), full));
            self.tail_alerts = Some(Tail::new(path.join("alerts.log"), full));
        } else {
            self.tail_gpu = None;
            self.tail_mem = None;
            self.tail_proc = None;
            self.tail_events = None;
            self.tail_wer = None;
            self.tail_alerts = None;
        }
        self.gpu_rows.clear();
        self.mem_points.clear();
        self.event_lines.clear();
        self.wer_lines.clear();
        self.alert_lines.clear();
        self.alert_count = 0;
        self.hb_fail = 0;
        self.last_gpu_summary = "—".to_string();
        self.last_mem_summary.clear();
        self.last_proc_summary.clear();
        self.title_dirty = true;
    }

    fn sync_from_live(&mut self) {
        let snap = crate::live::snapshot();
        let to_sev = |s: &str| match s {
            "bad" => Severity::Bad,
            "warn" => Severity::Warn,
            _ => Severity::Normal,
        };
        let to_entry = |it: crate::live::LiveLogItem| LogEntry {
            time: it.time,
            level: it.level,
            source: it.source,
            message: it.message,
            raw: it.raw,
            sev: to_sev(it.sev),
        };

        self.gpu_rows = snap
            .gpu_rows
            .into_iter()
            .map(|r| {
                let delta = match (r.temp.trim().parse::<f64>(), r.hotspot.trim().parse::<f64>()) {
                    (Ok(t), Ok(h)) => format!("{:.1}", h - t),
                    _ => String::new(),
                };
                GpuRow {
                    ts: r.ts,
                    temp: r.temp,
                    hotspot: r.hotspot,
                    hotspot_delta: delta,
                    power: r.power,
                    core: r.core,
                    memclk: r.memclk,
                    pcie: if r.pcie_drop { format!("{} (降速)", r.pcie) } else { r.pcie },
                    util: r.util,
                    vram: if r.vram_pct > 0.0 { format!("{} ({:.0}%)", r.vram, r.vram_pct) } else { r.vram },
                    fan: r.fan,
                    thr_label: if r.thr_label.is_empty() { "—".to_string() } else { r.thr_label },
                    ok: r.ok,
                    sev: to_sev(r.sev),
                }
            })
            .collect();
        self.mem_points = snap
            .mem_points
            .into_iter()
            .map(|m| MemPoint {
                ts: m.ts,
                commit_pct: m.commit_pct,
                nonpaged_mb: m.nonpaged_mb as f64,
            })
            .collect();
        self.event_lines = snap.events.into_iter().map(to_entry).collect();
        self.wer_lines = snap.wer.into_iter().map(to_entry).collect();
        self.alert_lines = snap.alerts.into_iter().map(to_entry).collect();
        self.last_gpu_summary = snap.gpu_summary;
        self.last_mem_summary = snap.mem_summary;
        self.last_proc_summary = snap.proc_summary;
        self.alert_count = snap.alert_count;
        if snap.alert_count > self.last_alert_notified {
            if self.last_alert_notified > 0 || snap.running {
                if let Some(latest) = self.alert_lines.back() {
                    self.live_banner_msg = Some((
                        format!("[{}] {}", latest.level, latest.message),
                        Instant::now() + Duration::from_secs(10),
                    ));
                }
            }
            self.last_alert_notified = snap.alert_count;
        } else if snap.alert_count == 0 {
            self.last_alert_notified = 0;
        }
        self.live_cpu = snap.cpu;
        self.live_cpu_points = snap.cpu_points;
        self.live_stats = snap.stats;
        self.live_env = snap.env;
    }

    fn tick(&mut self) {
        let was_collector = self.collector_running;
        let alive = running_pid().is_some();
        let starting = self.starting_until.is_some_and(|t| Instant::now() < t);
        if alive {
            self.starting_until = None;
        }
        self.collector_running = alive || starting;
        if !self.collector_running {
            self.stopping_until = None;
        }

        let live_now = crate::live::is_running();
        if live_now && !self.live_running {
            self.live_viewing = true;
            self.paused = false;
        }
        self.live_running = live_now;

        if alive && !was_collector && !live_now {
            self.awaiting_new_session = true;
            self.live_viewing = false;
            self.paused = false;
        } else if alive && !live_now && !self.live_viewing {
            if let Some(latest) = resolve_session(None) {
                if self.session.as_ref() != Some(&latest) {
                    self.sessions = list_sessions();
                    self.selected_idx = self.sessions.iter().position(|p| p == &latest).unwrap_or(0);
                    self.switch_session(latest);
                    return;
                }
            }
        }

        if self.awaiting_new_session {
            self.sessions = list_sessions();
            if let Some(latest) = resolve_session(None) {
                if self.session.as_ref() != Some(&latest) {
                    self.awaiting_new_session = false;
                    self.live_viewing = false;
                    self.selected_idx =
                        self.sessions.iter().position(|p| p == &latest).unwrap_or(0);
                    self.switch_session(latest);
                    return;
                }
            }
        }

        if self.live_viewing {
            if !self.paused {
                self.sync_from_live();
            }
        } else if !self.paused && self.session.is_some() {
            self.pull_gpu();
            self.pull_mem();
            self.pull_proc();
            self.pull_events();
            self.pull_wer();
            self.pull_alerts();
        }
        if self.last_incidents_scan.elapsed() >= Duration::from_secs(5) {
            self.refresh_incidents();
        }
        self.update_header();
    }

    fn pull_gpu(&mut self) {
        let Some(ref mut tail) = self.tail_gpu else {
            return;
        };
        let gpu_rows = &mut self.gpu_rows;
        let summary = &mut self.last_gpu_summary;
        let max_rows = if self.full_history {
            MAX_GPU_ROWS_FULL
        } else {
            MAX_GPU_ROWS_NORMAL
        };
        tail.read_new_with(|line| {
            if line.starts_with("ts,") {
                return;
            }
            let p: Vec<&str> = line.split(',').collect();
            if p.len() < 16 {
                return;
            }
            let (ts_raw, temp, power, core, memc, gen, wid, util, vram, fan, thr, ok) = (
                p[0], p[1], p[2], p[4], p[5], p[6], p[7], p[8], p[10], p[12], p[13], p[14],
            );
            let hotspot = p.get(17).map(|s| s.trim()).unwrap_or("");

            let bad = ok.trim() != "OK";
            let mut warn = false;
            let temp_f = temp.trim().parse::<f64>().ok();
            let hotspot_f = hotspot.parse::<f64>().ok();
            let power_f = power.trim().parse::<f64>().ok();
            let gen_i = gen.trim().parse::<i64>().ok();
            if temp_f.is_some_and(|t| t >= 75.0)
                || hotspot_f.is_some_and(|h| h >= 92.0)
                || match (temp_f, hotspot_f) {
                    (Some(t), Some(h)) => (h - t) >= 20.0,
                    _ => false,
                }
                || power_f.is_some_and(|w| w >= 350.0)
            {
                warn = true;
            }
            if power_f.is_some_and(|w| w > 150.0) && gen_i.is_some_and(|g| g < 4) {
                warn = true;
            }
            let sev = if bad {
                Severity::Bad
            } else if warn {
                Severity::Warn
            } else {
                Severity::Normal
            };

            let pcie = if !gen.trim().is_empty() {
                format!("Gen{}x{}", gen.trim(), wid.trim())
            } else {
                "—".to_string()
            };
            let thr_short = if thr.trim().is_empty() {
                String::new()
            } else {
                let hex = thr.trim().trim_start_matches("0x").trim_start_matches("0X");
                if let Ok(v) = u64::from_str_radix(hex, 16) {
                    format!("0x{:x}", v)
                } else {
                    thr.chars().take(10).collect()
                }
            };
            let ts_short = if ts_raw.len() > 19 {
                ts_raw[11..19].to_string()
            } else {
                ts_raw.to_string()
            };

            let hotspot_delta = match (temp_f, hotspot_f) {
                (Some(t), Some(h)) => format!("{:.1}", h - t),
                _ => "—".to_string(),
            };
            let temp_disp = if hotspot.is_empty() {
                format!("{}°C", temp)
            } else if hotspot_delta != "—" {
                format!("{}°C(热点{}°C/Δ{}°C)", temp, hotspot, hotspot_delta)
            } else {
                format!("{}°C(热点{}°C)", temp, hotspot)
            };
            *summary = format!(
                "{}  {}W  {}MHz  {}  util {}%  VRAM {}MB",
                temp_disp, power, core, pcie, util, vram
            );

            if gpu_rows.len() >= max_rows {
                gpu_rows.pop_front();
            }
            gpu_rows.push_back(GpuRow {
                ts: ts_short,
                temp: temp.to_string(),
                hotspot: if hotspot.is_empty() {
                    "—".to_string()
                } else {
                    hotspot.to_string()
                },
                hotspot_delta,
                power: power.to_string(),
                core: core.to_string(),
                memclk: memc.to_string(),
                pcie,
                util: util.to_string(),
                vram: vram.to_string(),
                fan: fan.to_string(),
                thr_label: {
                    let lbl = crate::infra::decode_throttle_label(&thr_short);
                    if lbl.is_empty() { "—".to_string() } else { lbl }
                },
                ok: ok.to_string(),
                sev,
            });
        });
    }

    fn pull_mem(&mut self) {
        let Some(ref mut tail) = self.tail_mem else {
            return;
        };
        let mem_points = &mut self.mem_points;
        let summary = &mut self.last_mem_summary;
        tail.read_new_with(|line| {
            if line.starts_with("ts,") {
                return;
            }
            let p: Vec<&str> = line.split(',').collect();
            if p.len() < 8 {
                return;
            }
            let ts_short = if p[0].len() >= 19 {
                p[0][11..19].to_string()
            } else {
                p[0].to_string()
            };
            let commit = p[1].trim().parse::<f64>().unwrap_or(0.0);
            let limit = p[2].trim().parse::<f64>().unwrap_or(0.0);
            let nonpaged = p[7].trim().parse::<f64>().unwrap_or(0.0);
            let pct = if limit > 0.0 {
                (commit / limit) * 100.0
            } else {
                0.0
            };
            *summary = format!("内存 {:.1}/{:.1}GB ({:.0}%)", commit, limit, pct);
            if mem_points.len() >= 300 {
                mem_points.pop_front();
            }
            mem_points.push_back(MemPoint {
                ts: ts_short,
                commit_pct: pct,
                nonpaged_mb: nonpaged,
            });
        });
    }

    fn pull_proc(&mut self) {
        let Some(ref mut tail) = self.tail_proc else {
            return;
        };
        let summary = &mut self.last_proc_summary;
        tail.read_new_with(|line| {
            if line.starts_with("ts,") {
                return;
            }
            let p: Vec<&str> = line.split(',').collect();
            if p.len() < 5 {
                return;
            }
            if p[1].trim() == "-" {
                *summary = "游戏 未运行".to_string();
            } else {
                *summary = format!("游戏 {} ({}MB)", p[1].trim(), p[3].trim());
            }
        });
    }

    fn pull_events(&mut self) {
        let Some(ref mut tail) = self.tail_events else {
            return;
        };
        let event_lines = &mut self.event_lines;
        tail.read_new_with(|ln| {
            if ln.starts_with("ts,") {
                return;
            }
            let p: Vec<&str> = ln.splitn(7, ',').collect();
            if p.len() < 7 {
                return;
            }
            let (ts, lg, lvl, eid, prov, _rid, msg) = (p[0], p[1], p[2], p[3], p[4], p[5], p[6]);
            let is_bad = matches!(lvl, "Error" | "Critical")
                || matches!(eid, "41" | "4101" | "4102" | "1001" | "117" | "219");
            let sev = if is_bad {
                Severity::Bad
            } else if lvl == "Warning" {
                Severity::Warn
            } else {
                Severity::Normal
            };
            let ts_short = if ts.len() >= 19 { &ts[11..19] } else { ts };
            let msg_clean = msg.trim_matches('"').to_string();
            if event_lines.len() >= MAX_LOG_LINES {
                event_lines.pop_front();
            }
            event_lines.push_back(LogEntry {
                time: ts_short.to_string(),
                level: if lvl.is_empty() {
                    "Info".to_string()
                } else {
                    lvl.to_string()
                },
                source: format!("{} · {} #{}", lg, prov, eid),
                message: msg_clean,
                raw: ln.to_string(),
                sev,
            });
        });
    }

    fn pull_wer(&mut self) {
        let Some(ref mut tail) = self.tail_wer else {
            return;
        };
        let wer_lines = &mut self.wer_lines;
        tail.read_new_with(|ln| {
            if ln.starts_with("ts,") {
                return;
            }
            let p: Vec<&str> = ln.splitn(5, ',').collect();
            let up = ln.to_uppercase();
            let sev = if up.contains("KERNEL") || up.contains("LIVEDUMP") {
                Severity::Bad
            } else {
                Severity::Warn
            };
            let (time, level, source, message) = if p.len() >= 5 {
                let ts_short = if p[0].len() >= 19 {
                    &p[0][11..19]
                } else {
                    p[0]
                };
                (
                    ts_short.to_string(),
                    p[1].to_string(),
                    p[3].to_string(),
                    format!("{} ({})", p[2], p[4]),
                )
            } else {
                (
                    String::new(),
                    "WER".to_string(),
                    String::new(),
                    ln.to_string(),
                )
            };
            if wer_lines.len() >= MAX_LOG_LINES {
                wer_lines.pop_front();
            }
            wer_lines.push_back(LogEntry {
                time,
                level,
                source,
                message,
                raw: ln.to_string(),
                sev,
            });
        });
    }

    fn pull_alerts(&mut self) {
        let Some(ref mut tail) = self.tail_alerts else {
            return;
        };
        let alert_lines = &mut self.alert_lines;
        let alert_count = &mut self.alert_count;
        tail.read_new_with(|ln| {
            *alert_count += 1;
            let bad = ["[INCIDENT]", "SMI_FAIL", "PCIE", "GAME_EXIT", "WER_NEW"]
                .iter()
                .any(|k| ln.contains(k));
            let sev = if bad { Severity::Bad } else { Severity::Warn };
            let (time, level, message) = if let Some((ts_part, rest)) = ln.split_once(' ') {
                let ts_short = if ts_part.len() >= 19 {
                    &ts_part[11..19]
                } else {
                    ts_part
                };
                if let Some(rest_trim) = rest.strip_prefix('[') {
                    if let Some((tag, msg)) = rest_trim.split_once(']') {
                        (
                            ts_short.to_string(),
                            tag.to_string(),
                            msg.trim().to_string(),
                        )
                    } else {
                        (ts_short.to_string(), "ALERT".to_string(), rest.to_string())
                    }
                } else {
                    (ts_short.to_string(), "ALERT".to_string(), rest.to_string())
                }
            } else {
                (String::new(), "ALERT".to_string(), ln.to_string())
            };
            if alert_lines.len() >= MAX_LOG_LINES {
                alert_lines.pop_front();
            }
            alert_lines.push_back(LogEntry {
                time,
                level,
                source: "告警引擎".to_string(),
                message,
                raw: ln.to_string(),
                sev,
            });
        });
    }

    fn refresh_incidents(&mut self) {
        self.last_incidents_scan = Instant::now();
        let session_key = self
            .session
            .as_ref()
            .map(|p| p.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let mut list = Vec::new();
        if let Ok(entries) = fs::read_dir(incidents_dir()) {
            for e in entries.flatten() {
                let dir = e.path();
                if !dir.is_dir() {
                    continue;
                }
                let name = e.file_name().to_string_lossy().into_owned();
                let mut belongs = false;
                let mut reason = String::new();
                if let Ok(raw) = fs::read_to_string(dir.join("incident.json")) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                        reason = v
                            .get("reason")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        let owner = v.get("session").and_then(|s| s.as_str()).unwrap_or("");
                        belongs = !session_key.is_empty() && owner.to_lowercase() == session_key;
                    }
                }
                list.push(IncidentEntry {
                    name,
                    dir,
                    belongs_to_session: belongs,
                    reason,
                });
            }
        }
        list.sort_by(|a, b| b.name.cmp(&a.name));
        self.incidents_cache = list;
        if self.selected_incident >= self.incidents_cache.len() {
            self.selected_incident = 0;
        }
        self.load_incident_preview();
    }

    fn load_incident_preview(&mut self) {
        let Some(inc) = self.incidents_cache.get(self.selected_incident) else {
            self.incident_preview_text.clear();
            return;
        };
        let target = if self.incident_preview_file == "Report_*.wer" {
            fs::read_dir(&inc.dir).ok().and_then(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .find(|p| p.extension().is_some_and(|x| x == "wer"))
            })
        } else {
            Some(inc.dir.join(&self.incident_preview_file))
        };

        let Some(path) = target else {
            self.incident_preview_text = "(该事故目录下暂无 Report.wer 文件)".to_string();
            return;
        };
        match fs::read(&path) {
            Ok(bytes) => {
                // Report.wer 在 Windows 上有时是 UTF-16LE
                let text = if bytes.starts_with(&[0xFF, 0xFE]) {
                    let u16s: Vec<u16> = bytes[2..]
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    String::from_utf16_lossy(&u16s)
                } else {
                    String::from_utf8_lossy(&bytes).into_owned()
                };
                self.incident_preview_text = text.lines().take(200).collect::<Vec<_>>().join("\n");
            }
            Err(_) => {
                self.incident_preview_text = format!("(文件不存在或无权读取：{})", path.display());
            }
        }
    }

    fn update_header(&mut self) {
        if self.live_viewing {
            self.heartbeat = if self.live_running {
                HeartbeatState {
                    text: tr(self.lang, "● 实时监测中（仅内存日志）", "● Live Monitor (Memory Only)").to_string(),
                    color: COL_OK,
                }
            } else {
                HeartbeatState {
                    text: tr(self.lang, "● 实时监测已停止（保留内存日志）", "● Live Monitor Stopped").to_string(),
                    color: COL_WARN,
                }
            };
            return;
        }

        if !self.collector_running {
            self.heartbeat = HeartbeatState {
                text: if self.session.is_some() {
                    tr(self.lang, "● 未在采集（查看历史）", "● Idle (Viewing History)").to_string()
                } else {
                    tr(self.lang, "● 未在采集（暂无会话）", "● Idle (No Session)").to_string()
                },
                color: COL_DIM,
            };
            return;
        }

        let Some(ref session) = self.session else {
            self.heartbeat = HeartbeatState {
                text: tr(self.lang, "● 采集器初始化中...", "● Initializing...").to_string(),
                color: COL_WARN,
            };
            return;
        };

        let hb_path = session.join("heartbeat.txt");
        let age = fs::read_to_string(&hb_path).ok().and_then(|s| {
            DateTime::parse_from_rfc3339(s.trim()).ok().map(|dt| {
                Local::now()
                    .signed_duration_since(dt.with_timezone(&Local))
                    .num_seconds() as f64
            })
        });
        if age.is_none() {
            self.hb_fail += 1;
            if self.hb_fail >= 3 || !hb_path.exists() {
                self.heartbeat = HeartbeatState {
                    text: tr(self.lang, "● 无心跳（采集器未运行）", "● No Heartbeat (Stopped)").to_string(),
                    color: COL_BAD,
                };
            }
            return;
        }
        self.hb_fail = 0;
        self.heartbeat = match age {
            None => unreachable!(),
            Some(a) if a < 5.0 => HeartbeatState {
                text: tr(self.lang, "● 采集器运行中", "● Collector Running").to_string(),
                color: COL_OK,
            },
            Some(a) => HeartbeatState {
                text: if matches!(self.lang, UiLang::Zh) {
                    format!("● 心跳延迟 {:.0}s", a)
                } else {
                    format!("● Heartbeat Lag {:.0}s", a)
                },
                color: COL_WARN,
            },
        };
    }

    /// 趋势曲线页：一行两个参数，含 8 项显卡指标 + 2 项内存/内核池指标，并标出告警线
    fn render_trend(
        ui: &mut egui::Ui,
        lang: UiLang,
        rows: &VecDeque<GpuRow>,
        mem_points: &VecDeque<MemPoint>,
        alerts: &VecDeque<LogEntry>,
        window: &mut usize,
        paused: &mut bool,
    ) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr(lang, "时间窗口：", "Window:")).color(COL_DIM).size(13.0));
            for (n, label) in [
                (15usize, tr(lang, "15 秒", "15s")),
                (20, tr(lang, "20 秒", "20s")),
                (30, tr(lang, "30 秒", "30s")),
                (60, tr(lang, "60 秒", "60s")),
            ] {
                if ui.selectable_label(*window == n, label).clicked() {
                    *window = n;
                }
            }
            if ui.button(tr(lang, "定位到末尾（黑屏前）", "Jump to Latest")).clicked() {
                *paused = false;
                *window = 30;
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new(tr(lang, "点上方标数值 · 红色虚线为告警发生时刻", "Hover for values · Red dashed line = Alert"))
                    .color(COL_DIM)
                    .size(12.0),
            );
        });
        ui.add_space(6.0);

        let start = rows.len().saturating_sub(*window);
        let slice: Vec<&GpuRow> = rows.iter().skip(start).collect();
        if slice.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new(tr(lang, "暂无 GPU 数据", "No GPU data")).size(15.0).color(COL_DIM));
            });
            return;
        }
        let times: Vec<String> = slice.iter().map(|r| r.ts.clone()).collect();
        let x_max = (*window as f64 - 0.5).max(slice.len() as f64 - 0.5);

        // 收集落在当前时间窗口内的告警位置（按 HH:MM:SS 匹配 X 轴）
        let alert_times: HashSet<&str> = alerts.iter().map(|a| a.time.as_str()).collect();
        let alert_xs: Vec<f64> = times
            .iter()
            .enumerate()
            .filter(|(_, t)| alert_times.contains(t.as_str()))
            .map(|(i, _)| i as f64)
            .collect();

        // 构造所有 10 张图表卡片（8 项 GPU + 2 项内存）
        let mut cards: Vec<CardData> = trend_metrics(lang)
            .into_iter()
            .map(|m| {
                let pts: Vec<[f64; 2]> = slice
                    .iter()
                    .enumerate()
                    .filter_map(|(i, r)| (m.get)(r).map(|v| [i as f64, v]))
                    .collect();
                CardData {
                    name: m.name,
                    unit: m.unit,
                    decimals: m.decimals,
                    color: m.color,
                    pts,
                    times: times.clone(),
                    x_max,
                    alert_xs: alert_xs.clone(),
                }
            })
            .collect();

        // 内存提交率与非分页内核池（按自身采样点，最多取最近 20 个点）
        let m_start = mem_points.len().saturating_sub((*window).min(30));
        let m_slice: Vec<&MemPoint> = mem_points.iter().skip(m_start).collect();
        let m_times: Vec<String> = m_slice.iter().map(|m| m.ts.clone()).collect();
        let m_xmax = (m_slice.len().max(10) as f64) - 0.5;
        let m_alert_xs: Vec<f64> = m_times
            .iter()
            .enumerate()
            .filter(|(_, t)| alert_times.contains(t.as_str()))
            .map(|(i, _)| i as f64)
            .collect();

        cards.push(CardData {
            name: tr(lang, "系统内存提交率", "Commit Charge"),
            unit: "%",
            decimals: 1,
            color: Color32::from_rgb(0x52, 0xc4, 0x1a),
            pts: m_slice
                .iter()
                .enumerate()
                .map(|(i, m)| [i as f64, m.commit_pct])
                .collect(),
            times: m_times.clone(),
            x_max: m_xmax,
            alert_xs: m_alert_xs.clone(),
        });
        cards.push(CardData {
            name: tr(lang, "非分页内核池", "Non-Paged Pool"),
            unit: "MB",
            decimals: 0,
            color: Color32::from_rgb(0xfa, 0x8c, 0x16),
            pts: m_slice
                .iter()
                .enumerate()
                .map(|(i, m)| [i as f64, m.nonpaged_mb])
                .collect(),
            times: m_times,
            x_max: m_xmax,
            alert_xs: m_alert_xs,
        });

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for pair in cards.chunks(2) {
                    ui.columns(2, |cols| {
                        Self::render_trend_card(&mut cols[0], &pair[0], lang);
                        if let Some(c2) = pair.get(1) {
                            Self::render_trend_card(&mut cols[1], c2, lang);
                        }
                    });
                    ui.add_space(10.0);
                }
            });
    }

    /// 渲染单个参数的趋势卡片
    fn render_trend_card(ui: &mut egui::Ui, c: &CardData, lang: UiLang) {
        let (ymin, ymax) = c.pts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p[1]), hi.max(p[1]))
        });
        let cur_str = c
            .pts
            .last()
            .map(|p| fmt_val(p[1], c.decimals))
            .unwrap_or_else(|| "—".to_string());

        let card_bg = Color32::from_rgb(0x13, 0x18, 0x22);
        let card_border = Color32::from_rgb(0x26, 0x31, 0x42);

        egui::Frame::none()
            .fill(card_bg)
            .stroke(Stroke::new(1.0_f32, card_border))
            .rounding(8.0)
            .inner_margin(egui::Margin::symmetric(12.0, 10.0))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), 22.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(4.0, 16.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, c.color);

                            let mut left_job = egui::text::LayoutJob::default();
                            left_job.append(
                                c.name,
                                0.0,
                                egui::TextFormat {
                                    font_id: egui::FontId::proportional(14.0),
                                    color: COL_FG,
                                    valign: egui::Align::Center,
                                    ..Default::default()
                                },
                            );
                            left_job.append(
                                &format!("  ({})", c.unit),
                                0.0,
                                egui::TextFormat {
                                    font_id: egui::FontId::proportional(12.0),
                                    color: COL_DIM,
                                    valign: egui::Align::Center,
                                    ..Default::default()
                                },
                            );
                            ui.label(left_job);

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let mut right_job = egui::text::LayoutJob::default();
                                    right_job.append(
                                        &format!("{} {}", cur_str, c.unit),
                                        0.0,
                                        egui::TextFormat {
                                            font_id: egui::FontId::proportional(14.5),
                                            color: c.color,
                                            valign: egui::Align::Center,
                                            ..Default::default()
                                        },
                                    );
                                    if !c.pts.is_empty() {
                                        right_job.append(
                                            &format!(
                                                "   {} {}  {} {}",
                                                tr(lang, "高", "Hi"),
                                                fmt_val(ymax, c.decimals),
                                                tr(lang, "低", "Lo"),
                                                fmt_val(ymin, c.decimals)
                                            ),
                                            0.0,
                                            egui::TextFormat {
                                                font_id: egui::FontId::proportional(12.0),
                                                color: COL_DIM,
                                                valign: egui::Align::Center,
                                                ..Default::default()
                                            },
                                        );
                                    }
                                    ui.label(right_job);
                                },
                            );
                        },
                    );

                    ui.add_space(4.0);

                    let plot_w = ui.available_width().max(120.0);
                    if c.pts.is_empty() {
                        ui.allocate_ui(egui::vec2(plot_w, 165.0), |ui| {
                            ui.centered_and_justified(|ui| {
                                ui.label(RichText::new(tr(lang, "该参数暂无有效数据", "No valid data")).color(COL_DIM));
                            });
                        });
                        return;
                    }

                    let span = ymax - ymin;
                    let pad_bot = (span * 0.18).max(ymax.abs() * 0.03).max(0.8);
                    let pad_top = (span * 0.36).max(ymax.abs() * 0.06).max(1.2);
                    let label_dy = (span + pad_bot + pad_top) * 0.04;
                    let y_floor = (ymin - pad_bot) as f32;
                    let times = c.times.clone();
                    let unit = c.unit;
                    let decimals = c.decimals;

                    Plot::new(c.name)
                        .height(165.0)
                        .width(plot_w)
                        .allow_drag(false)
                        .allow_zoom(false)
                        .allow_scroll(false)
                        .allow_boxed_zoom(false)
                        .allow_double_click_reset(false)
                        .y_axis_min_width(40.0_f32)
                        .include_x(-0.6)
                        .include_x(c.x_max)
                        .include_y(ymin - pad_bot)
                        .include_y(ymax + pad_top)
                        .x_axis_formatter(move |mark, _| {
                            let i = mark.value.round();
                            if (mark.value - i).abs() < 1e-6 && i >= 0.0 {
                                times
                                    .get(i as usize)
                                    .map(|s| s.get(3..).unwrap_or(s).to_string())
                                    .unwrap_or_default()
                            } else {
                                String::new()
                            }
                        })
                        .label_formatter(move |_, p| format!("{} {}", fmt_val(p.y, decimals), unit))
                        .show(ui, |pui| {
                            for &ax in &c.alert_xs {
                                pui.vline(
                                    VLine::new(ax)
                                        .color(COL_BAD)
                                        .style(LineStyle::Dashed { length: 4.0 })
                                        .width(1.2_f32),
                                );
                            }
                            pui.line(
                                Line::new(PlotPoints::from(c.pts.clone()))
                                    .color(c.color)
                                    .width(2.2_f32)
                                    .fill(y_floor),
                            );
                            pui.points(
                                Points::new(PlotPoints::from(c.pts.clone()))
                                    .color(c.color)
                                    .radius(3.2_f32),
                            );
                            for p in &c.pts {
                                pui.text(
                                    Text::new(
                                        PlotPoint::new(p[0], p[1] + label_dy),
                                        RichText::new(fmt_val(p[1], decimals))
                                            .size(9.5)
                                            .color(COL_FG),
                                    )
                                    .anchor(egui::Align2::CENTER_BOTTOM),
                                );
                            }
                        });
                });
            });
    }

    fn render_log_table(
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        lang: UiLang,
        lines: &VecDeque<LogEntry>,
        only_bad: bool,
        filter: &mut String,
        stick_bottom: bool,
        source_title: &str,
        status_hint: &mut Option<(String, Color32, Instant)>,
    ) {
        let filter_lower = filter.trim().to_lowercase();
        let filtered: Vec<&LogEntry> = lines
            .iter()
            .filter(|l| (!only_bad || l.sev != Severity::Normal) && l.matches(&filter_lower))
            .collect();

        ui.horizontal(|ui| {
            ui.label(RichText::new(tr(lang, "筛选：", "Filter:")).color(COL_DIM).size(13.0));
            ui.add(
                egui::TextEdit::singleline(filter)
                    .desired_width(260.0)
                    .hint_text(tr(lang, "输入关键词（时间 / 来源 / 级别 / 消息）", "Keyword (time / source / level / msg)")),
            );
            if !filter.is_empty() && ui.button(tr(lang, "清空", "Clear")).clicked() {
                filter.clear();
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new(if matches!(lang, UiLang::Zh) {
                    format!(
                        "显示 {} / {} 条（点击行可复制）",
                        filtered.len(),
                        lines.len()
                    )
                } else {
                    format!(
                        "Showing {} / {} (click row to copy)",
                        filtered.len(),
                        lines.len()
                    )
                })
                .color(COL_DIM)
                .size(12.0),
            );
        });
        ui.add_space(4.0);

        let heads = [
            (tr(lang, "时间", "Time"), 76.0),
            (tr(lang, "级别", "Level"), 96.0),
            (source_title, 220.0),
            (tr(lang, "详情 / 消息", "Details / Message"), 300.0),
        ];
        let mut table = TableBuilder::new(ui)
            .striped(false)
            .resizable(true)
            .stick_to_bottom(stick_bottom)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
        for (i, &(_, w)) in heads.iter().enumerate() {
            if i == heads.len() - 1 {
                table = table.column(Column::remainder().at_least(w));
            } else {
                table = table.column(Column::initial(w).at_least(60.0));
            }
        }
        table
            .header(24.0, |mut header| {
                for &(h, _) in &heads {
                    header.col(|ui| {
                        let rect = ui.available_rect_before_wrap();
                        ui.painter().rect_filled(rect, 0.0, COL_HEAD);
                        ui.label(RichText::new(h).color(COL_FG).strong().size(13.0));
                    });
                }
            })
            .body(|body| {
                body.rows(22.0, filtered.len(), |mut row| {
                    let item = filtered[row.index()];
                    let text_col = item.sev.color();
                    let (badge_bg, badge_fg) = match item.sev {
                        Severity::Bad => (Color32::from_rgb(0x6b, 0x1d, 0x22), Color32::WHITE),
                        Severity::Warn => (Color32::from_rgb(0x5c, 0x3d, 0x10), COL_WARN),
                        Severity::Normal => (Color32::from_rgb(0x1f, 0x2b, 0x3a), COL_FG),
                    };

                    row.col(|ui| {
                        ui.label(
                            RichText::new(&item.time)
                                .monospace()
                                .size(12.5)
                                .color(COL_DIM),
                        );
                    });
                    row.col(|ui| {
                        egui::Frame::none()
                            .fill(badge_bg)
                            .rounding(4.0)
                            .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(&item.level)
                                        .size(11.5)
                                        .strong()
                                        .color(badge_fg),
                                );
                            });
                    });
                    row.col(|ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&item.source).size(12.5).color(COL_FG))
                                .truncate(),
                        );
                    });
                    row.col(|ui| {
                        let resp = ui.add(
                            egui::Label::new(
                                RichText::new(&item.message)
                                    .monospace()
                                    .size(12.5)
                                    .color(text_col),
                            )
                            .truncate(),
                        );
                        resp.on_hover_text(&item.message);
                    });

                    if row.response().clicked() {
                        ctx.copy_text(item.raw.clone());
                        *status_hint = Some((
                            "已复制该条日志".to_string(),
                            COL_OK,
                            Instant::now() + Duration::from_secs(2),
                        ));
                    }
                });
            });
    }

    /// 告警与取证直览页：上部告警列表，下部事故取证目录列表与文件预览
    fn render_alerts_and_incidents(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let total_h = ui.available_height();
        let top_h = (total_h * 0.52).max(160.0);

        ui.allocate_ui(egui::vec2(ui.available_width(), top_h), |ui| {
            Self::render_log_table(
                ui,
                ctx,
                self.lang,
                &self.alert_lines,
                false,
                &mut self.log_filter,
                !self.paused,
                tr(self.lang, "来源", "Source"),
                &mut self.status_hint,
            );
        });

        ui.separator();

        let lang = self.lang;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if matches!(lang, UiLang::Zh) {
                    format!(
                        "事故取证直览（共 {} 条）",
                        self.incidents_cache.len()
                    )
                } else {
                    format!(
                        "Incident Forensics ({} bundles)",
                        self.incidents_cache.len()
                    )
                })
                .size(14.0)
                .strong()
                .color(COL_FG),
            );
            if ui.button(tr(lang, "刷新取证", "Refresh")).clicked() {
                self.refresh_incidents();
            }
            for fname in [
                "incident.json",
                "Report_*.wer",
                "wer-list.txt",
                "nvidia-smi-q.txt",
            ] {
                if ui
                    .selectable_label(self.incident_preview_file == fname, fname)
                    .clicked()
                {
                    self.incident_preview_file = fname.to_string();
                    self.load_incident_preview();
                }
            }
            if let Some(inc) = self.incidents_cache.get(self.selected_incident) {
                if ui.button(tr(lang, "打开该事故文件夹", "Open Incident Folder")).clicked() {
                    let _ = open_folder_in_explorer(&inc.dir);
                }
            }
        });
        ui.add_space(4.0);

        ui.columns(2, |cols| {
            cols[0].set_max_width(340.0);
            egui::ScrollArea::vertical()
                .id_salt("inc_list_scroll")
                .auto_shrink([false, false])
                .show(&mut cols[0], |ui| {
                    if self.incidents_cache.is_empty() {
                        ui.label(RichText::new(tr(lang, "暂无事故取证记录", "No incident records")).color(COL_DIM));
                        return;
                    }
                    let mut picked = None;
                    for (idx, inc) in self.incidents_cache.iter().enumerate() {
                        let tag = if inc.belongs_to_session {
                            tr(lang, "[本次] ", "[Cur] ")
                        } else {
                            ""
                        };
                        let label = format!("{}{}  {}", tag, inc.name, inc.reason);
                        if ui
                            .selectable_label(idx == self.selected_incident, label)
                            .clicked()
                        {
                            picked = Some(idx);
                        }
                    }
                    if let Some(i) = picked {
                        self.selected_incident = i;
                        self.load_incident_preview();
                    }
                });

            egui::ScrollArea::both()
                .id_salt("inc_preview_scroll")
                .auto_shrink([false, false])
                .show(&mut cols[1], |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&self.incident_preview_text)
                                .monospace()
                                .size(12.0)
                                .color(COL_FG),
                        )
                        .wrap(),
                    );
                });
        });
    }

    /// 参数设置弹窗
    fn render_settings_dialog(&mut self, ctx: &egui::Context) {
        let lang = self.lang;
        let Some(ref mut draft) = self.settings_draft else {
            return;
        };
        let mut open = true;
        let mut save_clicked = false;
        let mut cancel_clicked = false;

        egui::Window::new("采集与界面设置")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(440.0);
                egui::Grid::new("settings_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(tr(self.lang, "界面语言：", "Language:"));
                        ui.horizontal(|ui| {
                            let is_zh = draft.cfg.language != "en";
                            if ui.selectable_label(is_zh, "简体中文").clicked() {
                                draft.cfg.language = "zh-CN".to_string();
                                self.lang = UiLang::Zh;
                            }
                            if ui.selectable_label(!is_zh, "English").clicked() {
                                draft.cfg.language = "en".to_string();
                                self.lang = UiLang::En;
                            }
                        });
                        ui.end_row();

                        ui.label(tr(self.lang, "目标监控显卡：", "Target GPU:"));
                        if draft.gpus.is_empty() {
                            ui.add(egui::DragValue::new(&mut draft.cfg.gpu_index).range(0..=7));
                        } else {
                            let cur_name = draft
                                .gpus
                                .iter()
                                .find(|(i, _)| *i == draft.cfg.gpu_index)
                                .map(|(i, n)| format!("#{} {}", i, n))
                                .unwrap_or_else(|| format!("#{}", draft.cfg.gpu_index));
                            egui::ComboBox::from_id_salt("gpu_idx_pick")
                                .selected_text(cur_name)
                                .show_ui(ui, |ui| {
                                    for (i, n) in &draft.gpus {
                                        ui.selectable_value(
                                            &mut draft.cfg.gpu_index,
                                            *i,
                                            format!("#{} {}", i, n),
                                        );
                                    }
                                });
                        }
                        ui.end_row();

                        ui.label(tr(self.lang, "监控游戏进程（逗号分隔）：", "Game Processes (comma-separated):"));
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.game_names_text)
                                .desired_width(260.0),
                        );
                        ui.end_row();

                        ui.label(tr(self.lang, "温度告警阈值 (°C)：", "GPU Temp Warn (°C):"));
                        ui.add(
                            egui::DragValue::new(&mut draft.cfg.temp_warn)
                                .speed(0.5)
                                .range(40.0..=110.0),
                        );
                        ui.end_row();

                        ui.label(tr(self.lang, "热点告警阈值 (°C)：", "Hotspot Warn (°C):"));
                        ui.add(
                            egui::DragValue::new(&mut draft.cfg.hotspot_warn)
                                .speed(0.5)
                                .range(45.0..=120.0),
                        );
                        ui.end_row();

                        ui.label(tr(lang, "温差告警阈值 (°C)：", "Delta Warn (°C):"));
                        ui.add(
                            egui::DragValue::new(&mut draft.cfg.hotspot_delta_warn)
                                .speed(0.5)
                                .range(5.0..=50.0),
                        );
                        ui.end_row();

                        ui.label(tr(lang, "功耗告警阈值 (W)：", "Power Warn (W):"));
                        ui.add(
                            egui::DragValue::new(&mut draft.cfg.power_warn)
                                .speed(5.0)
                                .range(50.0..=1000.0),
                        );
                        ui.end_row();

                        ui.label(tr(lang, "CPU 温度告警阈值 (°C)：", "CPU Temp Warn (°C):"));
                        ui.add(
                            egui::DragValue::new(&mut draft.cfg.cpu_temp_warn)
                                .speed(0.5)
                                .range(50.0..=115.0),
                        );
                        ui.end_row();

                        ui.label(tr(lang, "CPU 占用告警阈值 (%)：", "CPU Usage Warn (%):"));
                        ui.add(
                            egui::DragValue::new(&mut draft.cfg.cpu_usage_warn)
                                .speed(1.0)
                                .range(50.0..=100.0),
                        );
                        ui.end_row();

                        ui.label(tr(lang, "显存占用告警阈值 (%)：", "VRAM Usage Warn (%):"));
                        ui.add(
                            egui::DragValue::new(&mut draft.cfg.vram_warn_pct)
                                .speed(1.0)
                                .range(50.0..=100.0),
                        );
                        ui.end_row();

                        ui.label(tr(lang, "PCIe 高负载掉速预警：", "PCIe Drop Alert:"));
                        ui.checkbox(&mut draft.cfg.pcie_drop_warn, tr(lang, "高负载下带宽低于硬件上限时告警", "Warn on PCIe downgrade under load"));
                        ui.end_row();

                        ui.label(tr(lang, "远程访问口令 (留空不限制)：", "Remote PIN (blank = off):"));
                        ui.add(egui::TextEdit::singleline(&mut draft.cfg.auth_pin).password(true).desired_width(140.0));
                        ui.end_row();

                        ui.label(tr(lang, "远程只读访客模式：", "Read-Only Remote Mode:"));
                        ui.checkbox(&mut draft.cfg.read_only_remote, tr(lang, "禁止远程修改配置/启停采集", "Disable remote controls"));
                        ui.end_row();

                        ui.label(tr(lang, "GPU 采样间隔 (秒)：", "GPU Interval (s):"));
                        ui.add(
                            egui::DragValue::new(&mut draft.cfg.gpu_interval)
                                .speed(0.1)
                                .range(0.5..=60.0),
                        );
                        ui.end_row();

                        ui.label(tr(lang, "内存/进程/WER 间隔 (秒)：", "Mem/Proc/WER Interval (s):"));
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut draft.cfg.mem_interval)
                                    .speed(0.5)
                                    .range(1.0..=60.0),
                            );
                            ui.add(
                                egui::DragValue::new(&mut draft.cfg.proc_interval)
                                    .speed(0.5)
                                    .range(1.0..=60.0),
                            );
                            ui.add(
                                egui::DragValue::new(&mut draft.cfg.wer_interval)
                                    .speed(0.5)
                                    .range(1.0..=60.0),
                            );
                        });
                        ui.end_row();

                        ui.label(tr(lang, "会话切分时长 (小时，0 不切分)：", "Session Split Hours (0 = off):"));
                        ui.add(
                            egui::DragValue::new(&mut draft.cfg.max_session_hours).range(0..=168),
                        );
                        ui.end_row();

                        ui.label(tr(lang, "单文件轮转体积 (MB)：", "File Rotate Size (MB):"));
                        ui.add(egui::DragValue::new(&mut draft.cfg.rotate_mb).range(8..=1024));
                        ui.end_row();

                        ui.label(tr(lang, "关闭窗口时最小化到托盘：", "Minimize to Tray on Close:"));
                        ui.checkbox(&mut draft.cfg.minimize_to_tray, tr(lang, "启用托盘常驻", "Enable tray resident"));
                        ui.end_row();

                        ui.label(tr(lang, "网页服务端口 (1024~65535)：", "Web Port (1024~65535):"));
                        ui.add(egui::DragValue::new(&mut draft.cfg.web_port).range(1024..=65535));
                        ui.end_row();
                    });

                ui.add_space(10.0);
                ui.label(
                    RichText::new(tr(lang, "提示：采集参数修改后将在下一次启动采集时生效", "Note: Capture settings take effect on next capture start"))
                        .size(12.0)
                        .color(COL_DIM),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let save_btn =
                        egui::Button::new(RichText::new(tr(lang, "保存设置", "Save")).color(Color32::WHITE))
                            .fill(Color32::from_rgb(0x1f, 0x6e, 0x43));
                    if ui.add(save_btn).clicked() {
                        save_clicked = true;
                    }
                    if ui.button(tr(lang, "取消", "Cancel")).clicked() {
                        cancel_clicked = true;
                    }
                });
            });

        if save_clicked {
            if let Some(mut d) = self.settings_draft.take() {
                d.cfg.game_names = d
                    .game_names_text
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if d.cfg.web_port < 1024 {
                    d.cfg.web_port = WEB_PORT;
                }
                let port_changed = d.cfg.web_port != self.web_port;
                self.minimize_to_tray = d.cfg.minimize_to_tray;
                self.web_port = d.cfg.web_port;
                self.status_hint = Some(match save_config(&d.cfg) {
                    Ok(_) => {
                        if port_changed && self.web_server.is_some() {
                            if let Some(mut old_ws) = self.web_server.take() {
                                old_ws.stop();
                            }
                            match start_web_server(self.web_port) {
                                Ok(new_ws) => {
                                    let url = new_ws.lan_url.clone();
                                    self.web_server = Some(new_ws);
                                    (
                                        if matches!(lang, UiLang::Zh) {
                                            format!("设置已保存，网页服务已切换至 {}", url)
                                        } else {
                                            format!("Settings saved; web UI switched to {}", url)
                                        },
                                        COL_OK,
                                        Instant::now() + Duration::from_secs(4),
                                    )
                                }
                                Err(e) => (e, COL_BAD, Instant::now() + Duration::from_secs(5)),
                            }
                        } else {
                            (
                                tr(lang, "设置已保存", "Settings saved").to_string(),
                                COL_OK,
                                Instant::now() + Duration::from_secs(3),
                            )
                        }
                    }
                    Err(e) => (e, COL_BAD, Instant::now() + Duration::from_secs(4)),
                });
            }
        } else if cancel_clicked || !open {
            self.settings_draft = None;
        }
    }

    /// 删除会话确认弹窗
    fn render_delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(target) = self.pending_delete.clone() else {
            return;
        };
        let lang = self.lang;
        let name = target
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut open = true;
        let mut decided: Option<bool> = None;
        egui::Window::new(tr(lang, "确认删除会话", "Delete Session"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(RichText::new(if matches!(lang, UiLang::Zh) {
                    format!("将永久删除会话 {} 的全部数据：", name)
                } else {
                    format!("Permanently delete all data for session {}:", name)
                }).size(14.0));
                ui.label(tr(lang, "· 会话目录内的 GPU / 内存 / 事件 / WER / 进程 / 告警记录", "· GPU / Memory / Event / WER / Process / Alert logs in session folder"));
                ui.label(tr(lang, "· 该会话产生的事故取证目录（incidents）", "· Incident forensics folders (incidents) created by this session"));
                ui.label(RichText::new(tr(lang, "删除后无法恢复。", "This action cannot be undone.")).color(COL_BAD).strong());
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let del = egui::Button::new(RichText::new(tr(lang, "确认删除", "Delete")).color(Color32::WHITE))
                        .fill(Color32::from_rgb(0x8c, 0x2d, 0x2d));
                    if ui.add(del).clicked() {
                        decided = Some(true);
                    }
                    if ui.button(tr(lang, "取消", "Cancel")).clicked() {
                        decided = Some(false);
                    }
                });
            });
        match decided {
            Some(true) => {
                self.pending_delete = None;
                self.delete_session(target);
            }
            Some(false) => self.pending_delete = None,
            None if !open => self.pending_delete = None,
            None => {}
        }
    }

    fn delete_session(&mut self, target: PathBuf) {
        let lang = self.lang;
        if running_pid().is_some() && latest_session_path().as_ref() == Some(&target) {
            self.status_hint = Some((
                tr(lang, "该会话正在采集中，请先停止采集再删除", "Session is active; stop capture before deleting").to_string(),
                COL_BAD,
                Instant::now() + Duration::from_secs(4),
            ));
            return;
        }
        let viewing = self.session.as_ref() == Some(&target);
        if viewing {
            self.tail_gpu = None;
            self.tail_mem = None;
            self.tail_proc = None;
            self.tail_events = None;
            self.tail_wer = None;
            self.tail_alerts = None;
        }
        let name = target
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.status_hint = Some(match delete_session_data(&target) {
            Ok(n) => (
                if matches!(lang, UiLang::Zh) {
                    format!("已删除会话 {}（含 {} 个事故目录）", name, n)
                } else {
                    format!("Deleted session {} ({} incidents)", name, n)
                },
                COL_OK,
                Instant::now() + Duration::from_secs(3),
            ),
            Err(e) => (e, COL_BAD, Instant::now() + Duration::from_secs(5)),
        });

        self.sessions = list_sessions();
        self.refresh_incidents();
        if viewing {
            if let Some(first) = self.sessions.first().cloned() {
                self.selected_idx = 0;
                self.switch_session(first);
            } else {
                self.session = None;
                self.selected_idx = 0;
                self.reset_tails(false);
            }
        } else {
            self.selected_idx = self
                .session
                .as_ref()
                .and_then(|s| self.sessions.iter().position(|p| p == s))
                .unwrap_or(0);
        }
    }

    /// 批量删除会话管理弹窗
    fn render_batch_delete_dialog(&mut self, ctx: &egui::Context) {
        if !self.batch_delete_open {
            return;
        }
        let mut open = true;
        let active = if self.collector_running {
            latest_session_path()
        } else {
            None
        };
        let mut do_delete = false;
        let mut do_close = false;

        let lang = self.lang;
        egui::Window::new(tr(lang, "批量删除会话", "Batch Delete Sessions"))
            .collapsible(false)
            .resizable(true)
            .default_size([460.0, 420.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(tr(lang, "全选", "All")).clicked() {
                        for p in &self.sessions {
                            if active.as_ref() != Some(p) {
                                self.batch_selected.insert(p.clone());
                            }
                        }
                    }
                    if ui.button(tr(lang, "取消全选", "None")).clicked() {
                        self.batch_selected.clear();
                    }
                    if ui.button(tr(lang, "仅选非当前", "Others")).clicked() {
                        self.batch_selected.clear();
                        for p in &self.sessions {
                            if active.as_ref() != Some(p) && self.session.as_ref() != Some(p) {
                                self.batch_selected.insert(p.clone());
                            }
                        }
                    }
                    if ui.button(tr(lang, "反选", "Invert")).clicked() {
                        for p in &self.sessions {
                            if active.as_ref() == Some(p) {
                                continue;
                            }
                            if !self.batch_selected.remove(p) {
                                self.batch_selected.insert(p.clone());
                            }
                        }
                    }
                });

                ui.add_space(4.0);
                ui.label(
                    RichText::new(tr(lang, "注意：删除将同时清理会话目录及关联的事故取证目录，不可恢复。", "Note: Deletes session logs and linked incident folders permanently."))
                        .size(12.0)
                        .color(COL_DIM),
                );
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("batch_del_scroll")
                    .max_height(280.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.sessions.is_empty() {
                            ui.label(RichText::new(tr(lang, "暂无可删除的会话", "No sessions available")).color(COL_DIM));
                            return;
                        }
                        for p in &self.sessions {
                            let is_active = active.as_ref() == Some(p);
                            let is_viewing = self.session.as_ref() == Some(p);
                            let name = p
                                .file_name()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            let size_str = dir_size_label(p);

                            ui.horizontal(|ui| {
                                let mut checked = !is_active && self.batch_selected.contains(p);
                                if ui
                                    .add_enabled(!is_active, egui::Checkbox::new(&mut checked, ""))
                                    .changed()
                                {
                                    if checked {
                                        self.batch_selected.insert(p.clone());
                                    } else {
                                        self.batch_selected.remove(p);
                                    }
                                }
                                ui.label(RichText::new(name).monospace().size(13.0).color(COL_FG));
                                if is_active {
                                    ui.label(RichText::new(tr(lang, "[采集中]", "[Active]")).size(11.5).color(COL_OK));
                                } else if is_viewing {
                                    ui.label(
                                        RichText::new(tr(lang, "[当前查看]", "[Current]")).size(11.5).color(COL_ACCENT),
                                    );
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(size_str)
                                                .monospace()
                                                .size(12.0)
                                                .color(COL_DIM),
                                        );
                                    },
                                );
                            });
                        }
                    });

                ui.separator();
                let count = self.batch_selected.len();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if matches!(lang, UiLang::Zh) {
                        format!("已选中 {} 个会话", count)
                    } else {
                        format!("{} selected", count)
                    }).color(COL_FG));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tr(lang, "关闭", "Close")).clicked() {
                            do_close = true;
                        }
                        let del_btn = egui::Button::new(
                            RichText::new(if matches!(lang, UiLang::Zh) {
                                format!("删除所选 ({})", count)
                            } else {
                                format!("Delete ({})", count)
                            }).color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(0x8c, 0x2d, 0x2d));
                        if ui.add_enabled(count > 0, del_btn).clicked() {
                            do_delete = true;
                        }
                    });
                });
            });

        if do_delete {
            let targets: Vec<PathBuf> = self.batch_selected.drain().collect();
            self.batch_delete_sessions(targets);
            self.batch_delete_open = false;
        } else if do_close || !open {
            self.batch_delete_open = false;
        }
    }

    fn batch_delete_sessions(&mut self, targets: Vec<PathBuf>) {
        let active = if running_pid().is_some() {
            latest_session_path()
        } else {
            None
        };
        let viewing_deleted = self
            .session
            .as_ref()
            .is_some_and(|cur| targets.contains(cur));
        if viewing_deleted {
            self.tail_gpu = None;
            self.tail_mem = None;
            self.tail_proc = None;
            self.tail_events = None;
            self.tail_wer = None;
            self.tail_alerts = None;
        }

        let mut ok_sessions = 0usize;
        let mut ok_incidents = 0usize;
        let mut fail_count = 0usize;
        for target in targets {
            if active.as_ref() == Some(&target) {
                fail_count += 1;
                continue;
            }
            match delete_session_data(&target) {
                Ok(n) => {
                    ok_sessions += 1;
                    ok_incidents += n;
                }
                Err(_) => fail_count += 1,
            }
        }

        self.status_hint = Some(if fail_count == 0 {
            (
                format!("已批量删除 {} 个会话（含 {} 个事故目录）", ok_sessions, ok_incidents),
                COL_OK,
                Instant::now() + Duration::from_secs(4),
            )
        } else {
            (
                format!("已删除 {} 个会话，{} 个失败或正在采集中", ok_sessions, fail_count),
                COL_WARN,
                Instant::now() + Duration::from_secs(5),
            )
        });

        self.sessions = list_sessions();
        self.refresh_incidents();
        if viewing_deleted {
            if let Some(first) = self.sessions.first().cloned() {
                self.selected_idx = 0;
                self.switch_session(first);
            } else {
                self.session = None;
                self.selected_idx = 0;
                self.reset_tails(false);
            }
        } else {
            self.selected_idx = self
                .session
                .as_ref()
                .and_then(|s| self.sessions.iter().position(|p| p == s))
                .unwrap_or(0);
        }
    }

    fn stop_web_and_resume(&mut self) {
        if let Some(mut ws) = self.web_server.take() {
            ws.stop();
        }
        // 同步网页端期间可能发生的配置、自启、会话增删与新开采集会话
        self.sessions = list_sessions();
        self.startup_enabled = is_startup_installed();
        self.minimize_to_tray = load_config().minimize_to_tray;

        let latest = resolve_session(None);
        let cur_missing = self.session.as_ref().is_none_or(|s| !s.is_dir());
        let has_newer = latest.is_some() && latest != self.session;

        if (cur_missing || has_newer) && latest.is_some() {
            if let Some(target) = latest {
                self.selected_idx = self
                    .sessions
                    .iter()
                    .position(|p| p == &target)
                    .unwrap_or(0);
                self.switch_session(target);
            }
        } else if cur_missing {
            self.session = None;
            self.selected_idx = 0;
            self.reset_tails(false);
            self.tick();
            self.refresh_incidents();
        } else {
            self.selected_idx = self
                .session
                .as_ref()
                .and_then(|s| self.sessions.iter().position(|p| p == s))
                .unwrap_or(0);
            self.tick();
            self.refresh_incidents();
        }
        self.status_hint = Some((
            "已关闭网页服务，主界面已同步恢复".to_string(),
            COL_OK,
            Instant::now() + Duration::from_secs(3),
        ));
    }

    fn handle_tray(&mut self, ctx: &egui::Context) {
        if let Some(ref t) = self.tray {
            if t.allow_quit.load(Ordering::SeqCst) {
                self.allow_quit = true;
            }
            for act in t.poll_actions() {
                match act {
                    TrayAction::ShowWindow => {
                        self.window_visible = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    TrayAction::HideWindow => {
                        self.window_visible = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    }
                    TrayAction::StartCollector => {
                        if start_supervisor_bg().is_ok() {
                            self.awaiting_new_session = true;
                            self.collector_running = true;
                            self.starting_until = Some(Instant::now() + Duration::from_secs(8));
                        }
                    }
                    TrayAction::StopCollector => {
                        request_stop_bg();
                        self.stopping_until = Some(Instant::now() + Duration::from_secs(6));
                    }
                    TrayAction::QuitUiOnly => {
                        self.allow_quit = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    TrayAction::QuitAll => {
                        self.allow_quit = true;
                        request_stop_bg();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        }

        if self.minimize_to_tray
            && !self.allow_quit
            && ctx.input(|i| i.viewport().close_requested())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.window_visible = false;
            set_main_window_visible(false);
        }
    }
}

struct CardData {
    name: &'static str,
    unit: &'static str,
    decimals: usize,
    color: Color32,
    pts: Vec<[f64; 2]>,
    times: Vec<String>,
    x_max: f64,
    alert_xs: Vec<f64>,
}

impl eframe::App for GpuCapApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_tray(ctx);

        // 网页服务端接管期间，暂停主界面自身的所有数据读取与定时刷新
        if self.web_server.is_none() {
            if !self.init_loaded {
                self.init_loaded = true;
                self.last_tick = Instant::now();
                self.tick();
                self.refresh_incidents();
            } else if self.last_tick.elapsed() >= Duration::from_secs(1) {
                self.last_tick = Instant::now();
                self.tick();
            }
            ctx.request_repaint_after(Duration::from_secs(1));
        }

        if self.title_dirty {
            self.title_dirty = false;
            let session_name = self
                .session
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "未启动".to_string());
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                "GPUCap 实时监控 — {}",
                session_name
            )));
        }

        if self.show_lang_picker {
            egui::Window::new("Choose Language / 请选择语言")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .fixed_size([380.0, 175.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Please select your interface language\n请选择您的界面语言")
                                .size(13.5)
                                .color(COL_FG),
                        );
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            ui.add_space(40.0);
                            if ui
                                .add_sized([130.0, 38.0], egui::Button::new(RichText::new("English").size(14.0).strong()))
                                .clicked()
                            {
                                self.lang = UiLang::En;
                                let mut cfg = load_config();
                                cfg.language = "en".to_string();
                                let _ = save_config(&cfg);
                                self.show_lang_picker = false;
                            }
                            ui.add_space(16.0);
                            if ui
                                .add_sized([130.0, 38.0], egui::Button::new(RichText::new("简体中文").size(14.0).strong()))
                                .clicked()
                            {
                                self.lang = UiLang::Zh;
                                let mut cfg = load_config();
                                cfg.language = "zh-CN".to_string();
                                let _ = save_config(&cfg);
                                self.show_lang_picker = false;
                            }
                        });
                    });
                });
        }

        if self.hud_mode {
            let mut restore_window = false;
            let mut close_hud = false;

            // 先布局右侧固定控制区，确保按钮永远独立且不遮挡左侧监控数据
            egui::SidePanel::right("hud_controls")
                .resizable(false)
                .show_separator_line(false)
                .frame(
                    egui::Frame::none()
                        .fill(Color32::from_rgb(0x0b, 0x0f, 0x17))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x23, 0x30, 0x44)))
                        .inner_margin(egui::Margin::symmetric(8.0, 6.0)),
                )
                .show(ctx, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let restore_btn = egui::Button::new(
                            RichText::new(tr(self.lang, "还原", "Restore"))
                                .size(11.5)
                                .color(Color32::from_rgb(0x38, 0xbd, 0xf8)),
                        )
                        .fill(Color32::from_rgb(0x16, 0x26, 0x3a))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x26, 0x43, 0x63)))
                        .rounding(6.0);
                        if ui.add(restore_btn).clicked() {
                            restore_window = true;
                        }
                        let close_btn = egui::Button::new(
                            RichText::new("×").size(13.0).color(COL_DIM),
                        )
                        .fill(Color32::from_rgb(0x18, 0x20, 0x2c))
                        .rounding(6.0);
                        if ui
                            .add(close_btn)
                            .on_hover_text(tr(self.lang, "退出程序", "Close"))
                            .clicked()
                        {
                            close_hud = true;
                        }
                    });
                });

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::none()
                        .fill(Color32::from_rgb(0x0b, 0x0f, 0x17))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x23, 0x30, 0x44)))
                        .inner_margin(egui::Margin::symmetric(12.0, 6.0)),
                )
                .show(ctx, |ui| {
                    // 悬浮条空白区支持鼠标左键直接拖拽移动，双击还原主窗口
                    let drag_resp = ui.interact(
                        ui.max_rect(),
                        ui.id().with("hud_drag_area"),
                        egui::Sense::click_and_drag(),
                    );
                    if drag_resp.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if drag_resp.double_clicked() {
                        restore_window = true;
                    }

                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;

                        // 状态圆点 + 品牌标识（替代冗长文字）
                        let (dot_rect, _) =
                            ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter()
                            .circle_filled(dot_rect.center(), 4.0, self.heartbeat.color);
                        ui.label(
                            RichText::new("GPUCap")
                                .size(12.0)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .on_hover_text(&self.heartbeat.text);

                        ui.separator();

                        let gpu_txt = self.last_gpu_summary.replace("util ", "");
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("GPU {}", gpu_txt))
                                    .size(12.0)
                                    .monospace()
                                    .color(COL_FG),
                            )
                            .truncate(),
                        );

                        if let Some(ref cpu) = self.live_cpu {
                            ui.separator();
                            let t_str = cpu
                                .temp_c
                                .map(|t| format!("{t:.0}°C"))
                                .unwrap_or_else(|| "—".into());
                            ui.label(
                                RichText::new(format!("CPU {:.0}% {}", cpu.total_usage, t_str))
                                    .size(12.0)
                                    .monospace()
                                    .color(Color32::from_rgb(0x38, 0xbd, 0xf8)),
                            );
                        }

                        if self.alert_count > 0 {
                            ui.separator();
                            ui.label(
                                RichText::new(if matches!(self.lang, UiLang::Zh) {
                                    format!("告警 {}", self.alert_count)
                                } else {
                                    format!("! {}", self.alert_count)
                                })
                                .size(11.5)
                                .strong()
                                .color(COL_WARN),
                            );
                        }
                    });
                });

            if restore_window {
                self.hud_mode = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                    egui::WindowLevel::Normal,
                ));
                ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                    1080.0, 620.0,
                )));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    1240.0, 820.0,
                )));
            } else if close_hud {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }

        // 顶部状态条（紧凑两行分层仪表盘：状态徽章 + 极值 + 指标胶囊）
        egui::TopBottomPanel::top("top_status")
            .frame(
                egui::Frame::none()
                    .fill(COL_PANEL)
                    .stroke(Stroke::new(1.0_f32, COL_BORDER))
                    .inner_margin(egui::Margin::symmetric(12.0, 6.0)),
            )
            .show(ctx, |ui| {
                // 第一行：采集状态胶囊 + 会话名 + 右侧告警/操作提示
                ui.horizontal(|ui| {
                    let (pill_bg, pill_border) = if self.heartbeat.color == COL_OK {
                        (
                            Color32::from_rgb(0x13, 0x2e, 0x22),
                            Color32::from_rgb(0x23, 0x5c, 0x41),
                        )
                    } else if self.heartbeat.color == COL_WARN {
                        (
                            Color32::from_rgb(0x36, 0x27, 0x10),
                            Color32::from_rgb(0x6e, 0x4d, 0x18),
                        )
                    } else if self.heartbeat.color == COL_DIM {
                        (
                            Color32::from_rgb(0x19, 0x20, 0x2b),
                            Color32::from_rgb(0x2b, 0x36, 0x48),
                        )
                    } else {
                        (
                            Color32::from_rgb(0x33, 0x16, 0x19),
                            Color32::from_rgb(0x6b, 0x24, 0x2a),
                        )
                    };
                    egui::Frame::none()
                        .fill(pill_bg)
                        .stroke(Stroke::new(1.0_f32, pill_border))
                        .rounding(12.0)
                        .inner_margin(egui::Margin::symmetric(10.0, 3.0))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(&self.heartbeat.text)
                                    .size(13.5)
                                    .strong()
                                    .color(self.heartbeat.color),
                            );
                        });

                    if let Some(ref s) = self.session {
                        let sname = s
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        ui.label(
                            RichText::new(if matches!(self.lang, UiLang::Zh) {
                                format!("当前会话 {}", sname)
                            } else {
                                format!("Session {}", sname)
                            })
                            .size(12.5)
                            .color(COL_DIM),
                        );
                    }
                    if let Some(ref env) = self.live_env {
                        let drv = if env.driver_version.is_empty() { "—" } else { &env.driver_version };
                        ui.label(
                            RichText::new(if matches!(self.lang, UiLang::Zh) {
                                format!("· {} · 驱动 {} · 电源 {}", env.gpu_name, drv, env.power_plan)
                            } else {
                                format!("· {} · Driver {} · Power {}", env.gpu_name, drv, env.power_plan)
                            })
                            .size(11.5)
                            .color(COL_DIM),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(tr(self.lang, "置顶浮条", "HUD")).clicked() {
                            self.hud_mode = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
                            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                                egui::WindowLevel::AlwaysOnTop,
                            ));
                            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(
                                egui::vec2(520.0, 38.0),
                            ));
                            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                                egui::vec2(780.0, 40.0),
                            ));
                        }
                        if self.alert_count > 0 {
                            egui::Frame::none()
                                .fill(Color32::from_rgb(0x3b, 0x24, 0x12))
                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x78, 0x49, 0x18)))
                                .rounding(10.0)
                                .inner_margin(egui::Margin::symmetric(10.0, 3.0))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(if matches!(self.lang, UiLang::Zh) {
                                            format!("告警 {} 条", self.alert_count)
                                        } else {
                                            format!("Alerts {}", self.alert_count)
                                        })
                                        .size(13.0)
                                        .strong()
                                        .color(COL_WARN),
                                    );
                                });
                        }
                        if let Some(ref ts) = self.live_stats.gpu_temp {
                            let p_max = self
                                .live_stats
                                .gpu_power
                                .as_ref()
                                .map(|p| {
                                    if matches!(self.lang, UiLang::Zh) {
                                        format!(" · {:.0}W峰", p.max)
                                    } else {
                                        format!(" · {:.0}W Peak", p.max)
                                    }
                                })
                                .unwrap_or_default();
                            ui.label(
                                RichText::new(if matches!(self.lang, UiLang::Zh) {
                                    format!("极值 {:.0}~{:.0}°C (均{:.0}°C){}", ts.min, ts.max, ts.avg, p_max)
                                } else {
                                    format!("Range {:.0}~{:.0}°C (Avg {:.0}°C){}", ts.min, ts.max, ts.avg, p_max)
                                })
                                .size(11.5)
                                .monospace()
                                .color(COL_DIM),
                            );
                        }
                        if self.copy_notice_until.is_some_and(|t| Instant::now() < t) {
                            ui.label(RichText::new(tr(self.lang, "✓ 已复制到剪贴板", "✓ Copied")).color(COL_OK).size(12.5));
                        } else if let Some((ref msg, col, until)) = self.status_hint {
                            if Instant::now() < until {
                                ui.label(RichText::new(msg).color(col).size(12.5));
                            }
                        }
                    });
                });

                ui.add_space(4.0);

                // 第二行：按比例铺满全宽的 3 张实时指标卡片（消灭右侧留白）
                let gap = 8.0_f32;
                let avail_w = ui.available_width();
                let w_gpu = ((avail_w - gap * 2.0) * 0.50).max(360.0);
                let w_mem = ((avail_w - gap * 2.0) * 0.26).max(180.0);
                let w_proc = (avail_w - gap * 2.0 - w_gpu - w_mem).max(160.0);

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    let render_chip = |ui: &mut egui::Ui,
                                       width: f32,
                                       tag: &str,
                                       tag_col: Color32,
                                       val: &str,
                                       val_col: Color32| {
                        let mut job = egui::text::LayoutJob::default();
                        job.append(
                            &format!("{}  ", tag),
                            0.0,
                            egui::TextFormat {
                                font_id: egui::FontId::proportional(12.5),
                                color: tag_col,
                                valign: egui::Align::Center,
                                ..Default::default()
                            },
                        );
                        job.append(
                            val,
                            0.0,
                            egui::TextFormat {
                                font_id: egui::FontId::proportional(12.5),
                                color: val_col,
                                valign: egui::Align::Center,
                                ..Default::default()
                            },
                        );
                        egui::Frame::none()
                            .fill(Color32::from_rgb(0x11, 0x16, 0x1f))
                            .stroke(Stroke::new(1.0_f32, COL_BORDER))
                            .rounding(6.0)
                            .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                            .show(ui, |ui| {
                                ui.set_width((width - 26.0).max(80.0));
                                ui.add(egui::Label::new(job).truncate());
                            });
                    };

                    render_chip(ui, w_gpu, tr(self.lang, "显卡", "GPU"), COL_ACCENT, &self.last_gpu_summary, COL_FG);

                    let mem_txt = if self.last_mem_summary.is_empty() {
                        "—"
                    } else {
                        self.last_mem_summary
                            .strip_prefix("内存 ")
                            .unwrap_or(&self.last_mem_summary)
                    };
                    render_chip(ui, w_mem, tr(self.lang, "内存", "RAM"), COL_OK, mem_txt, COL_FG);

                    let (proc_txt, pcol) = if self.last_proc_summary.is_empty() {
                        ("—", COL_DIM)
                    } else if self.last_proc_summary.contains("未运行") {
                        (
                            self.last_proc_summary
                                .strip_prefix("游戏 ")
                                .unwrap_or(&self.last_proc_summary),
                            COL_DIM,
                        )
                    } else {
                        (
                            self.last_proc_summary
                                .strip_prefix("游戏 ")
                                .unwrap_or(&self.last_proc_summary),
                            COL_OK,
                        )
                    };
                    render_chip(ui, w_proc, tr(self.lang, "进程", "Proc"), COL_WARN, proc_txt, pcol);
                });
            });

        // 未在实时监测时不展示 CPU 分栏
        if !(self.live_viewing && self.live_cpu.is_some()) && self.active_tab == Tab::Cpu {
            self.active_tab = Tab::Gpu;
        }

        // 工具栏与现代分段式标签栏
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::none()
                    .fill(COL_BG)
                    .inner_margin(egui::Margin::symmetric(12.0, 5.0)),
            )
            .show(ctx, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 30.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                    let stopping = self.stopping_until.is_some_and(|t| Instant::now() < t);
                    if self.collector_running {
                        let stop_btn = egui::Button::new(
                            RichText::new(if stopping {
                                tr(self.lang, "正在停止...", "Stopping...")
                            } else {
                                tr(self.lang, "■ 停止采集", "■ Stop")
                            })
                            .color(Color32::WHITE)
                            .strong()
                            .size(13.0),
                        )
                        .fill(Color32::from_rgb(0x9e, 0x2f, 0x2f));
                        if ui.add_enabled(!stopping, stop_btn).clicked() {
                            request_stop_bg();
                            self.stopping_until = Some(Instant::now() + Duration::from_secs(6));
                            self.awaiting_new_session = false;
                            self.status_hint = Some((
                                tr(self.lang, "已发送停止信号", "Stop signal sent").to_string(),
                                COL_WARN,
                                Instant::now() + Duration::from_secs(3),
                            ));
                        }
                    } else {
                        let start_btn = egui::Button::new(
                            RichText::new(tr(self.lang, "▶ 启动采集", "▶ Collect"))
                                .color(Color32::WHITE)
                                .strong()
                                .size(13.0),
                        )
                        .fill(Color32::from_rgb(0x1f, 0x7a, 0x4c));
                        if ui.add(start_btn).clicked() {
                            crate::live::stop_live();
                            self.live_viewing = false;
                            self.live_running = false;
                            match start_supervisor_bg() {
                                Ok(_) => {
                                    self.awaiting_new_session = true;
                                    self.collector_running = true;
                                    self.starting_until =
                                        Some(Instant::now() + Duration::from_secs(8));
                                    self.status_hint = Some((
                                        "后台采集已启动".to_string(),
                                        COL_OK,
                                        Instant::now() + Duration::from_secs(3),
                                    ));
                                }
                                Err(e) => {
                                    self.status_hint =
                                        Some((e, COL_BAD, Instant::now() + Duration::from_secs(4)));
                                }
                            }
                        }
                    }

                    ui.add_space(4.0);
                    if self.live_running {
                        let stop_live_btn = egui::Button::new(
                            RichText::new(tr(self.lang, "■ 停止监测", "■ Stop Live"))
                                .color(Color32::WHITE)
                                .strong()
                                .size(13.0),
                        )
                        .fill(Color32::from_rgb(0xb4, 0x53, 0x09));
                        if ui.add(stop_live_btn).clicked() {
                            crate::live::stop_live();
                            self.live_running = false;
                            self.live_viewing = true;
                            self.sync_from_live();
                            self.status_hint = Some((
                                tr(self.lang, "已停止实时监测（保留内存日志）", "Live monitor stopped (in-memory log kept)").to_string(),
                                COL_WARN,
                                Instant::now() + Duration::from_secs(3),
                            ));
                        }
                    } else {
                        let live_btn = egui::Button::new(
                            RichText::new(tr(self.lang, "◉ 实时监测", "◉ Live"))
                                .color(Color32::WHITE)
                                .strong()
                                .size(13.0),
                        )
                        .fill(Color32::from_rgb(0x02, 0x84, 0xc7));
                        if ui.add(live_btn).clicked() {
                            match crate::live::start_live() {
                                Ok(_) => {
                                    self.live_running = true;
                                    self.live_viewing = true;
                                    self.collector_running = false;
                                    self.awaiting_new_session = false;
                                    self.sync_from_live();
                                    self.status_hint = Some((
                                        tr(self.lang, "已开启纯内存实时监测（不保存磁盘）", "In-memory live monitor started").to_string(),
                                        COL_OK,
                                        Instant::now() + Duration::from_secs(3),
                                    ));
                                }
                                Err(e) => {
                                    self.status_hint =
                                        Some((e, COL_BAD, Instant::now() + Duration::from_secs(4)));
                                }
                            }
                        }
                    }

                    ui.separator();

                    let pause_label = if self.paused {
                        tr(self.lang, "继续滚动", "Resume")
                    } else {
                        tr(self.lang, "暂停滚动", "Pause")
                    };
                    if ui.button(pause_label).clicked() {
                        self.paused = !self.paused;
                    }
                    ui.checkbox(&mut self.only_bad, tr(self.lang, "只看异常", "Alerts Only"));

                    let mut auto_start = self.startup_enabled;
                    if ui.checkbox(&mut auto_start, tr(self.lang, "开机自启", "Auto Start")).changed() {
                        let res = if auto_start {
                            install_startup_reg()
                        } else {
                            uninstall_startup_reg()
                        };
                        match res {
                            Ok(_) => {
                                self.startup_enabled = auto_start;
                                self.status_hint = Some((
                                    if auto_start {
                                        "已开启开机自启".to_string()
                                    } else {
                                        "已关闭开机自启".to_string()
                                    },
                                    COL_OK,
                                    Instant::now() + Duration::from_secs(2),
                                ));
                            }
                            Err(e) => {
                                self.status_hint =
                                    Some((e, COL_BAD, Instant::now() + Duration::from_secs(4)));
                            }
                        }
                    }

                    let mut bg_run = self.minimize_to_tray;
                    let lang = self.lang;
                    if ui
                        .checkbox(&mut bg_run, tr(lang, "后台运行", "Background"))
                        .on_hover_text(tr(lang, "开启后点击右上角关闭按钮将缩入系统托盘；关闭则直接退出", "Minimize to tray on close instead of exiting"))
                        .changed()
                    {
                        self.minimize_to_tray = bg_run;
                        let mut cfg = load_config();
                        cfg.minimize_to_tray = bg_run;
                        self.status_hint = Some(match save_config(&cfg) {
                            Ok(_) => (
                                if bg_run {
                                    tr(lang, "已开启后台运行（关闭缩入托盘）", "Background tray enabled").to_string()
                                } else {
                                    tr(lang, "已关闭后台运行（关闭直接退出）", "Background tray disabled").to_string()
                                },
                                COL_OK,
                                Instant::now() + Duration::from_secs(3),
                            ),
                            Err(e) => (e, COL_BAD, Instant::now() + Duration::from_secs(4)),
                        });
                    }

                    if ui.button(tr(lang, "设置", "Settings")).clicked() {
                        let cfg = load_config();
                        let game_names_text = cfg.game_names.join(", ");
                        let gpus = GpuSampler::list_gpus();
                        self.settings_draft = Some(SettingsDraft {
                            cfg,
                            game_names_text,
                            gpus,
                        });
                    }

                    if self.web_server.is_some() {
                        let web_btn =
                            egui::Button::new(RichText::new(tr(lang, "关闭网页", "Stop Web")).color(Color32::WHITE))
                                .fill(Color32::from_rgb(0x1f, 0x7a, 0x4c));
                        if ui
                            .add(web_btn)
                            .on_hover_text(tr(lang, "关闭网页服务端并恢复主界面数据刷新", "Stop web server and resume desktop UI"))
                            .clicked()
                        {
                            self.stop_web_and_resume();
                        }
                    } else if ui
                        .button(tr(lang, "网页服务", "Web UI"))
                        .on_hover_text(if matches!(lang, UiLang::Zh) {
                            format!(
                                "开启网页端（IPv4 0.0.0.0:{0} + IPv6 [::]:{0}）并暂停主界面数据读取与刷新",
                                self.web_port
                            )
                        } else {
                            format!(
                                "Start web server (IPv4 0.0.0.0:{0} + IPv6 [::]:{0}) and pause desktop polling",
                                self.web_port
                            )
                        })
                        .clicked()
                    {
                        match start_web_server(self.web_port) {
                            Ok(ws) => {
                                let url = ws.lan_url.clone();
                                self.web_server = Some(ws);
                                self.status_hint = Some((
                                    if matches!(lang, UiLang::Zh) {
                                        format!("网页服务已开启：{}", url)
                                    } else {
                                        format!("Web server started: {}", url)
                                    },
                                    COL_OK,
                                    Instant::now() + Duration::from_secs(5),
                                ));
                            }
                            Err(e) => {
                                self.status_hint =
                                    Some((e, COL_BAD, Instant::now() + Duration::from_secs(5)));
                            }
                        }
                    }
                    if ui
                        .button(tr(lang, "退出", "Quit"))
                        .on_hover_text(tr(lang, "彻底关闭界面（不缩入托盘）", "Exit application completely"))
                        .clicked()
                    {
                        self.allow_quit = true;
                        if let Some(ref t) = self.tray {
                            t.allow_quit.store(true, Ordering::SeqCst);
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(tr(lang, "批量删除", "Batch Del"))
                            .on_hover_text(tr(lang, "批量勾选并清理历史会话数据", "Select and delete multiple sessions"))
                            .clicked()
                        {
                            self.sessions = list_sessions();
                            self.batch_selected.clear();
                            self.batch_delete_open = true;
                        }
                        let current_label = self
                            .sessions
                            .get(self.selected_idx)
                            .and_then(|p| p.file_name())
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| tr(lang, "(无会话)", "(No Sessions)").to_string());
                        let active = if self.collector_running {
                            latest_session_path()
                        } else {
                            None
                        };
                        let mut picked: Option<usize> = None;
                        let mut delete_req: Option<PathBuf> = None;
                        egui::ComboBox::from_id_salt("session_picker")
                            .width(185.0)
                            .selected_text(current_label)
                            .show_ui(ui, |ui| {
                                ui.set_min_width(260.0);
                                for (idx, p) in self.sessions.iter().enumerate() {
                                    let name = p
                                        .file_name()
                                        .map(|s| s.to_string_lossy().into_owned())
                                        .unwrap_or_default();
                                    ui.horizontal(|ui| {
                                        if ui
                                            .selectable_label(idx == self.selected_idx, name)
                                            .clicked()
                                        {
                                            picked = Some(idx);
                                        }
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if active.as_ref() == Some(p) {
                                                    ui.label(
                                                        RichText::new("采集中")
                                                            .size(11.5)
                                                            .color(COL_OK),
                                                    );
                                                } else if ui
                                                    .small_button(
                                                        RichText::new("删除").color(COL_BAD),
                                                    )
                                                    .on_hover_text("删除该会话的全部数据")
                                                    .clicked()
                                                {
                                                    delete_req = Some(p.clone());
                                                }
                                            },
                                        );
                                    });
                                }
                            });
                        if delete_req.is_some() {
                            self.pending_delete = delete_req;
                        }
                        if let Some(idx) = picked {
                            if idx != self.selected_idx {
                                self.selected_idx = idx;
                                if let Some(p) = self.sessions.get(idx).cloned() {
                                    self.switch_session(p);
                                }
                            }
                        }
                        ui.label(RichText::new(tr(lang, "会话", "Session")).color(COL_DIM).size(12.5));

                        if let Some(ref s) = self.session {
                            if ui.button(tr(lang, "打包导出", "Export ZIP")).clicked() {
                                match export_session_zip(s) {
                                    Ok(zip_path) => {
                                        let _ = open_folder_in_explorer(&data_dir());
                                        let zname = zip_path
                                            .file_name()
                                            .map(|n| n.to_string_lossy().into_owned())
                                            .unwrap_or_default();
                                        self.status_hint = Some((
                                            if matches!(lang, UiLang::Zh) {
                                                format!("已导出 {}", zname)
                                            } else {
                                                format!("Exported {}", zname)
                                            },
                                            COL_OK,
                                            Instant::now() + Duration::from_secs(5),
                                        ));
                                    }
                                    Err(e) => {
                                        self.status_hint = Some((
                                            e,
                                            COL_BAD,
                                            Instant::now() + Duration::from_secs(4),
                                        ));
                                    }
                                }
                            }
                            if ui.button(tr(lang, "复制路径", "Copy Path")).clicked() {
                                ctx.copy_text(s.to_string_lossy().into_owned());
                                self.copy_notice_until =
                                    Some(Instant::now() + Duration::from_secs(2));
                            }
                            if !self.full_history
                                && ui
                                    .button(tr(lang, "加载全部", "Load All"))
                                    .on_hover_text(tr(lang, "默认只读末尾 512KB，点击加载完整会话", "Loads tail 512KB by default; click to load full session"))
                                    .clicked()
                            {
                                self.reset_tails(true);
                                self.tick();
                            }
                        }
                        if ui.button(tr(lang, "打开目录", "Open Folder")).clicked() {
                            let target = self.session.clone().unwrap_or_else(data_dir);
                            if let Err(e) = open_folder_in_explorer(&target) {
                                self.status_hint =
                                    Some((e, COL_BAD, Instant::now() + Duration::from_secs(4)));
                            }
                        }
                    });
                    },
                );

                ui.add_space(8.0);
                // 分段式标签导航栏（带底部蓝色指示条）
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let mut tab_items = vec![(Tab::Gpu, tr(self.lang, "实时 GPU", "Live GPU"))];
                    if self.live_viewing && self.live_cpu.is_some() {
                        tab_items.push((Tab::Cpu, tr(self.lang, "CPU 监测", "CPU Monitor")));
                    }
                    tab_items.extend([
                        (Tab::Trend, tr(self.lang, "趋势曲线", "Trends")),
                        (Tab::Events, tr(self.lang, "系统事件", "Events")),
                        (Tab::Wer, tr(self.lang, "WER / 事故", "WER / Crashes")),
                        (Tab::Alerts, tr(self.lang, "告警与取证", "Alerts & Dumps")),
                    ]);
                    for (tab, title) in tab_items {
                        let selected = self.active_tab == tab;
                        let bg = if selected { COL_PANEL } else { COL_BG };
                        let fg = if selected { Color32::WHITE } else { COL_DIM };
                        let btn =
                            egui::Button::new(RichText::new(title).color(fg).size(13.5).strong())
                                .fill(bg)
                                .stroke(Stroke::new(
                                    1.0_f32,
                                    if selected {
                                        COL_BORDER
                                    } else {
                                        Color32::TRANSPARENT
                                    },
                                ))
                                .min_size(egui::vec2(104.0, 30.0));
                        let resp = ui.add(btn);
                        if selected {
                            let r = resp.rect;
                            ui.painter().line_segment(
                                [
                                    egui::pos2(r.left() + 8.0, r.bottom() - 1.5),
                                    egui::pos2(r.right() - 8.0, r.bottom() - 1.5),
                                ],
                                Stroke::new(2.5_f32, COL_ACCENT),
                            );
                        }
                        if resp.clicked() {
                            self.active_tab = tab;
                        }
                    }
                });
            });

        // 主内容区域（带细腻 1px 边框容器）
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(COL_BG)
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0)),
            )
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(COL_PANEL)
                    .stroke(Stroke::new(1.0_f32, COL_BORDER))
                    .rounding(8.0)
                    .inner_margin(egui::Margin::symmetric(12.0, 10.0))
                    .show(ui, |ui| {
                        if let Some(ref ws) = self.web_server {
                            let local_url = ws.local_url.clone();
                            let lan_url = ws.lan_url.clone();
                            let local_ipv6_url = ws.local_ipv6_url.clone();
                            let ipv6_url = ws.ipv6_url.clone();
                            let mut close_web = false;
                            ui.centered_and_justified(|ui| {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(40.0);
                                    let lang = self.lang;
                                    ui.label(
                                        RichText::new(tr(
                                            lang,
                                            "网页服务端已开启，主界面数据读取与刷新已暂停",
                                            "Web server is running; desktop UI polling is paused",
                                        ))
                                        .size(18.0)
                                        .strong()
                                        .color(COL_OK),
                                    );
                                    ui.add_space(10.0);
                                    ui.label(
                                        RichText::new(tr(
                                            lang,
                                            "后台采集守护进程（如已启动）不受影响，请通过浏览器查看实时监控",
                                            "Background capture (if started) continues unaffected. Open the web dashboard in a browser.",
                                        ))
                                        .size(13.5)
                                        .color(COL_DIM),
                                    );
                                    ui.add_space(18.0);
                                    egui::Frame::none()
                                        .fill(Color32::from_rgb(0x11, 0x16, 0x1f))
                                        .stroke(Stroke::new(1.0_f32, COL_BORDER))
                                        .rounding(8.0)
                                        .inner_margin(egui::Margin::symmetric(20.0, 14.0))
                                        .show(ui, |ui| {
                                            ui.label(
                                                RichText::new(format!("{}{}", tr(lang, "本机 IPv4：", "Local IPv4: "), local_url))
                                                    .size(14.0)
                                                    .color(COL_FG),
                                            );
                                            ui.add_space(4.0);
                                            ui.label(
                                                RichText::new(format!("{}{}", tr(lang, "局域网 IPv4：", "LAN IPv4: "), lan_url))
                                                    .size(14.0)
                                                    .color(COL_ACCENT),
                                            );
                                            ui.add_space(4.0);
                                            ui.label(
                                                RichText::new(tr(
                                                    lang,
                                                    "局域网 UDP 自动发现端口：8788（原生端可自动扫描）",
                                                    "LAN UDP discovery port: 8788 (auto-discovered by native apps)",
                                                ))
                                                .size(12.5)
                                                .color(COL_DIM),
                                            );
                                            ui.add_space(4.0);
                                            ui.label(
                                                RichText::new(format!("{}{}", tr(lang, "本机 IPv6：", "Local IPv6: "), local_ipv6_url))
                                                    .size(14.0)
                                                    .color(COL_FG),
                                            );
                                            ui.add_space(4.0);
                                            if let Some(ref v6) = ipv6_url {
                                                ui.label(
                                                    RichText::new(format!("{}{}", tr(lang, "外网 IPv6：", "Public IPv6: "), v6))
                                                        .size(14.0)
                                                        .color(COL_OK),
                                                );
                                            } else {
                                                ui.label(
                                                    RichText::new(if matches!(lang, UiLang::Zh) {
                                                        format!(
                                                            "外网 IPv6：未检测到公网 IPv6（已绑定 [::]:{}）",
                                                            self.web_port
                                                        )
                                                    } else {
                                                        format!(
                                                            "Public IPv6: None detected (bound [::]:{})",
                                                            self.web_port
                                                        )
                                                    })
                                                    .size(13.0)
                                                    .color(COL_DIM),
                                                );
                                            }
                                        });
                                    ui.add_space(18.0);
                                    ui.horizontal(|ui| {
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new(tr(lang, "在浏览器中打开", "Open in Browser"))
                                                        .color(Color32::WHITE),
                                                )
                                                .fill(Color32::from_rgb(0x1f, 0x6e, 0x43)),
                                            )
                                            .clicked()
                                        {
                                            ctx.open_url(egui::OpenUrl::new_tab(&local_url));
                                        }
                                        if ui.button(tr(lang, "复制局域网 IPv4", "Copy LAN IPv4")).clicked() {
                                            ctx.copy_text(lan_url.clone());
                                            self.copy_notice_until =
                                                Some(Instant::now() + Duration::from_secs(2));
                                        }
                                        if let Some(ref v6) = ipv6_url {
                                            if ui.button(tr(lang, "复制外网 IPv6", "Copy Public IPv6")).clicked() {
                                                ctx.copy_text(v6.clone());
                                                self.copy_notice_until =
                                                    Some(Instant::now() + Duration::from_secs(2));
                                            }
                                        }
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new(tr(lang, "关闭网页服务并恢复主界面", "Stop Web Server & Resume"))
                                                        .color(Color32::WHITE),
                                                )
                                                .fill(Color32::from_rgb(0x8c, 0x2d, 0x2d)),
                                            )
                                            .clicked()
                                        {
                                            close_web = true;
                                        }
                                    });
                                });
                            });
                            if close_web {
                                self.stop_web_and_resume();
                            }
                            return;
                        }
                        if self.session.is_none() {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    RichText::new(
                                        tr(self.lang, "暂无采集会话，请点击左上角「启动采集」开始后台记录", "No active session. Click 'Start Cap' in the top-left to begin."),
                                    )
                                    .size(15.0)
                                    .color(COL_DIM),
                                );
                            });
                            return;
                        }

                        if let Some((ref banner, until)) = self.live_banner_msg {
                            if Instant::now() < until {
                                egui::Frame::none()
                                    .fill(Color32::from_rgb(0x45, 0x16, 0x19))
                                    .stroke(Stroke::new(1.0_f32, COL_BAD))
                                    .rounding(6.0)
                                    .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(if matches!(self.lang, UiLang::Zh) {
                                                    format!("实时告警：{}", banner)
                                                } else {
                                                    format!("Live Alert: {}", banner)
                                                })
                                                .color(Color32::from_rgb(0xfc, 0xa5, 0xa5))
                                                .strong()
                                                .size(12.5),
                                            );
                                        });
                                    });
                                ui.add_space(4.0);
                            }
                        }

                        match self.active_tab {
                            Tab::Cpu => {
                                if let Some(ref cpu) = self.live_cpu {
                                    ui.horizontal(|ui| {
                                        if ui.button(tr(self.lang, "复制诊断摘要", "Copy Summary")).clicked() {
                                            let summary = crate::live::build_diagnostic_summary();
                                            ctx.copy_text(summary);
                                            self.live_action_msg = Some((
                                                tr(self.lang, "已复制 Markdown 诊断摘要到剪贴板", "Diagnostic Markdown summary copied").to_string(),
                                                COL_OK,
                                                Instant::now() + Duration::from_secs(4),
                                            ));
                                        }
                                        if !self.live_running {
                                            if ui.button(tr(self.lang, "补存为完整会话", "Save Session")).clicked() {
                                                match crate::live::save_snapshot_to_session() {
                                                    Ok(sess) => {
                                                        self.live_action_msg = Some((
                                                            if matches!(self.lang, UiLang::Zh) {
                                                                format!("已补存为会话：{}", sess)
                                                            } else {
                                                                format!("Saved as session: {}", sess)
                                                            },
                                                            COL_OK,
                                                            Instant::now() + Duration::from_secs(5),
                                                        ));
                                                    }
                                                    Err(err) => {
                                                        self.live_action_msg = Some((
                                                            err,
                                                            COL_BAD,
                                                            Instant::now() + Duration::from_secs(5),
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                        if let Some((ref msg, col, until)) = self.live_action_msg {
                                            if Instant::now() < until {
                                                ui.label(RichText::new(msg).color(col).size(12.0));
                                            }
                                        }
                                    });
                                    ui.add_space(4.0);
                                    render_cpu_ring_panel(
                                        ui,
                                        cpu,
                                        &self.live_cpu_points,
                                        &self.live_stats,
                                        &mut self.cpu_cores_expanded,
                                        self.lang,
                                    );
                                } else {
                                    ui.centered_and_justified(|ui| {
                                        ui.label(
                                            RichText::new(tr(self.lang, "请点击「实时监测」开始采集 CPU 数据", "Click 'Live Mon' to start CPU monitoring"))
                                                .color(COL_DIM)
                                                .size(13.0),
                                        );
                                    });
                                }
                            }
                            Tab::Gpu => {
                                let filtered: Vec<&GpuRow> = self
                                    .gpu_rows
                                    .iter()
                                    .filter(|r| !self.only_bad || r.sev != Severity::Normal)
                                    .collect();

                                let lang = self.lang;
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(if matches!(lang, UiLang::Zh) {
                                            format!(
                                                "已加载 {} 条显卡样本{}",
                                                self.gpu_rows.len(),
                                                if self.full_history {
                                                    "（完整历史）"
                                                } else {
                                                    ""
                                                }
                                            )
                                        } else {
                                            format!(
                                                "Loaded {} GPU samples{}",
                                                self.gpu_rows.len(),
                                                if self.full_history {
                                                    " (Full History)"
                                                } else {
                                                    ""
                                                }
                                            )
                                        })
                                        .size(12.0)
                                        .color(COL_DIM),
                                    );
                                    if ui.button(tr(lang, "跳到末尾（黑屏前）", "Jump to Latest")).clicked() {
                                        self.paused = false;
                                        self.scroll_gpu_bottom = true;
                                    }
                                });
                                ui.add_space(2.0);

                                let heads = if self.lang == UiLang::En {
                                    [
                                        ("Time", 84.0),
                                        ("Temp°C", 62.0),
                                        ("Hotspot°C", 68.0),
                                        ("Delta°C", 64.0),
                                        ("Power W", 66.0),
                                        ("Core MHz", 76.0),
                                        ("Mem MHz", 76.0),
                                        ("PCIe", 76.0),
                                        ("Util%", 62.0),
                                        ("VRAM MB", 74.0),
                                        ("Fan%", 58.0),
                                        ("Throttle", 92.0),
                                        ("Status", 62.0),
                                    ]
                                } else {
                                    [
                                        ("时间", 84.0),
                                        ("温度°C", 62.0),
                                        ("热点°C", 64.0),
                                        ("温差°C", 64.0),
                                        ("功耗W", 66.0),
                                        ("核心MHz", 76.0),
                                        ("显存MHz", 76.0),
                                        ("PCIe", 76.0),
                                        ("占用%", 62.0),
                                        ("显存MB", 74.0),
                                        ("风扇%", 58.0),
                                        ("降频原因", 92.0),
                                        ("状态", 62.0),
                                    ]
                                };
                                let mut table = TableBuilder::new(ui)
                                    .striped(false)
                                    .resizable(true)
                                    .stick_to_bottom(!self.paused)
                                    .cell_layout(egui::Layout::centered_and_justified(
                                        egui::Direction::LeftToRight,
                                    ));
                                if self.scroll_gpu_bottom {
                                    table = table.scroll_to_row(
                                        filtered.len().saturating_sub(1),
                                        Some(egui::Align::BOTTOM),
                                    );
                                    self.scroll_gpu_bottom = false;
                                }
                                for (i, &(_, w)) in heads.iter().enumerate() {
                                    if i == heads.len() - 1 {
                                        table = table.column(Column::remainder().at_least(w));
                                    } else {
                                        table = table.column(Column::initial(w).at_least(48.0));
                                    }
                                }
                                table
                                    .header(24.0, |mut header| {
                                        for &(h, _) in &heads {
                                            header.col(|ui| {
                                                let rect = ui.available_rect_before_wrap();
                                                ui.painter().rect_filled(rect, 0.0, COL_HEAD);
                                                ui.label(
                                                    RichText::new(h)
                                                        .color(COL_FG)
                                                        .strong()
                                                        .size(13.0),
                                                );
                                            });
                                        }
                                    })
                                    .body(|body| {
                                        body.rows(22.0, filtered.len(), |mut row| {
                                            let r = filtered[row.index()];
                                            let c = r.sev.color();
                                            let thr_txt = tr_thr(self.lang, &r.thr_label);
                                            for val in [
                                                &r.ts,
                                                &r.temp,
                                                &r.hotspot,
                                                &r.hotspot_delta,
                                                &r.power,
                                                &r.core,
                                                &r.memclk,
                                                &r.pcie,
                                                &r.util,
                                                &r.vram,
                                                &r.fan,
                                                &thr_txt,
                                                &r.ok,
                                            ] {
                                                row.col(|ui| {
                                                    ui.label(
                                                        RichText::new(val)
                                                            .monospace()
                                                            .size(13.0)
                                                            .color(c),
                                                    );
                                                });
                                            }
                                        });
                                    });
                            }
                            Tab::Trend => {
                                Self::render_trend(
                                    ui,
                                    self.lang,
                                    &self.gpu_rows,
                                    &self.mem_points,
                                    &self.alert_lines,
                                    &mut self.trend_window,
                                    &mut self.paused,
                                );
                            }
                            Tab::Events => {
                                Self::render_log_table(
                                    ui,
                                    ctx,
                                    self.lang,
                                    &self.event_lines,
                                    self.only_bad,
                                    &mut self.log_filter,
                                    !self.paused,
                                    tr(self.lang, "来源 / 事件编号", "Source / Event ID"),
                                    &mut self.status_hint,
                                );
                            }
                            Tab::Wer => {
                                Self::render_log_table(
                                    ui,
                                    ctx,
                                    self.lang,
                                    &self.wer_lines,
                                    false,
                                    &mut self.log_filter,
                                    !self.paused,
                                    tr(self.lang, "创建时间", "Created At"),
                                    &mut self.status_hint,
                                );
                            }
                            Tab::Alerts => {
                                self.render_alerts_and_incidents(ui, ctx);
                            }
                        }
                    });
            });

        self.render_settings_dialog(ctx);
        self.render_delete_dialog(ctx);
        self.render_batch_delete_dialog(ctx);
    }
}

/// 趋势曲线的参数定义
struct TrendMetric {
    name: &'static str,
    unit: &'static str,
    decimals: usize,
    color: Color32,
    get: fn(&GpuRow) -> Option<f64>,
}

fn parse_num(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

fn fmt_val(v: f64, decimals: usize) -> String {
    format!("{:.*}", decimals, v)
}

fn trend_metrics(lang: UiLang) -> Vec<TrendMetric> {
    vec![
        TrendMetric {
            name: tr(lang, "GPU 温度", "GPU Temp"),
            unit: "°C",
            decimals: 1,
            color: Color32::from_rgb(0xff, 0x7a, 0x45),
            get: |r| parse_num(&r.temp),
        },
        TrendMetric {
            name: tr(lang, "GPU 热点温度", "GPU Hotspot"),
            unit: "°C",
            decimals: 1,
            color: Color32::from_rgb(0xf4, 0x3f, 0x5e),
            get: |r| parse_num(&r.hotspot),
        },
        TrendMetric {
            name: tr(lang, "热点温差 (热点-GPU)", "Hotspot Delta"),
            unit: "°C",
            decimals: 1,
            color: Color32::from_rgb(0xfb, 0x92, 0x3c),
            get: |r| parse_num(&r.hotspot_delta),
        },
        TrendMetric {
            name: tr(lang, "功耗", "Power"),
            unit: "W",
            decimals: 1,
            color: Color32::from_rgb(0xff, 0xb0, 0x20),
            get: |r| parse_num(&r.power),
        },
        TrendMetric {
            name: tr(lang, "核心频率", "Core Clock"),
            unit: "MHz",
            decimals: 0,
            color: Color32::from_rgb(0x4d, 0xa3, 0xff),
            get: |r| parse_num(&r.core),
        },
        TrendMetric {
            name: tr(lang, "显存频率", "Mem Clock"),
            unit: "MHz",
            decimals: 0,
            color: Color32::from_rgb(0x9b, 0x7b, 0xff),
            get: |r| parse_num(&r.memclk),
        },
        TrendMetric {
            name: tr(lang, "GPU 占用", "GPU Util"),
            unit: "%",
            decimals: 0,
            color: Color32::from_rgb(0x3d, 0xdc, 0x84),
            get: |r| parse_num(&r.util),
        },
        TrendMetric {
            name: tr(lang, "显存占用", "VRAM Used"),
            unit: "MB",
            decimals: 0,
            color: Color32::from_rgb(0x36, 0xcf, 0xc9),
            get: |r| parse_num(&r.vram),
        },
        TrendMetric {
            name: tr(lang, "风扇转速", "Fan Speed"),
            unit: "%",
            decimals: 0,
            color: Color32::from_rgb(0xf7, 0x59, 0xab),
            get: |r| parse_num(&r.fan),
        },
        TrendMetric {
            name: tr(lang, "PCIe 代数", "PCIe Gen"),
            unit: "Gen",
            decimals: 0,
            color: Color32::from_rgb(0xff, 0x4d, 0x4f),
            get: |r| {
                r.pcie
                    .strip_prefix("Gen")
                    .and_then(|s| s.split('x').next())
                    .and_then(parse_num)
            },
        },
    ]
}

fn dir_size_label(dir: &PathBuf) -> String {
    let mut total: u64 = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            if let Ok(meta) = e.metadata() {
                if meta.is_file() {
                    total += meta.len();
                }
            }
        }
    }
    if total >= 1024 * 1024 {
        format!("{:.1} MB", total as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} KB", total as f64 / 1024.0)
    }
}

pub fn list_sessions() -> Vec<PathBuf> {
    let base = sessions_dir();
    let Ok(rd) = fs::read_dir(base) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort_by(|a, b| b.cmp(a));
    dirs
}

pub fn resolve_session(arg: Option<&str>) -> Option<PathBuf> {
    if let Some(s) = arg {
        let p = PathBuf::from(s);
        if p.is_dir() {
            return Some(p);
        }
    }
    if let Ok(raw) = fs::read_to_string(latest_file()) {
        let p = PathBuf::from(raw.trim());
        if p.is_dir() {
            return Some(p);
        }
    }
    list_sessions().into_iter().next()
}

pub fn cmd_ui(session_arg: Option<String>, selftest: bool, minimized: bool) -> i32 {
    let session = resolve_session(session_arg.as_deref());

    if selftest {
        match &session {
            Some(s) => println!("selftest ok: {}", s.display()),
            None => println!("selftest ok: no session yet ({})", data_dir().display()),
        }
        return 0;
    }

    // 单例检查：若已有界面实例，已发送唤醒信号将其拉至前台，当前进程直接退出
    let Some(singleton) = UiSingleton::acquire_or_wakeup() else {
        return 0;
    };
    let singleton = Arc::new(singleton);

    // 与图形后端初始化并行预读字体，显著缩短打开耗时
    let font_handle = thread::spawn(GpuCapApp::build_fonts);

    let session_label = session
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "未启动".to_string());
    let title = format!("GPUCap 实时监控 — {}", session_label);

    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    wgpu_options.present_mode = eframe::wgpu::PresentMode::AutoNoVsync;

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(&title)
        .with_inner_size([1360.0, 820.0])
        .with_min_inner_size([1080.0, 620.0])
        .with_visible(!minimized);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/gpucap-256.png")) {
        viewport = viewport.with_icon(Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        vsync: false,
        wgpu_options,
        ..Default::default()
    };

    let mut font_opt = Some(font_handle);
    let res = eframe::run_native(
        &title,
        options,
        Box::new(move |cc| {
            let preloaded = font_opt.take().and_then(|h| h.join().ok()).flatten();
            Ok(Box::new(GpuCapApp::new(
                cc, session, singleton, preloaded, minimized,
            )))
        }),
    );

    match res {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("启动界面失败：{}", e);
            1
        }
    }
}

fn draw_ring_gauge(
    ui: &mut egui::Ui,
    size: f32,
    stroke_w: f32,
    progress: f32,
    color: Color32,
    center_text: &str,
    font_size: f32,
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter();
    let center = rect.center();
    let radius = (size - stroke_w) * 0.5;
    let p = progress.clamp(0.0, 1.0);

    painter.circle_stroke(
        center,
        radius,
        Stroke::new(stroke_w, Color32::from_rgb(0x1e, 0x29, 0x3b)),
    );

    if p > 0.005 {
        let steps = ((p * 48.0).ceil() as usize).max(2);
        let start_ang = -std::f32::consts::FRAC_PI_2;
        let sweep = p * std::f32::consts::TAU;
        let pts: Vec<egui::Pos2> = (0..=steps)
            .map(|i| {
                let ang = start_ang + sweep * (i as f32 / steps as f32);
                egui::pos2(center.x + radius * ang.cos(), center.y + radius * ang.sin())
            })
            .collect();
        painter.add(egui::Shape::line(pts, Stroke::new(stroke_w, color)));
    }

    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        center_text,
        egui::FontId::monospace(font_size),
        Color32::WHITE,
    );
}

fn render_cpu_ring_panel(
    ui: &mut egui::Ui,
    cpu: &crate::cpu_probe::CpuSnapshot,
    cpu_points: &[crate::live::LiveCpuPoint],
    stats: &crate::live::LiveStatsSummary,
    cores_expanded: &mut bool,
    lang: UiLang,
) {
    let accent = Color32::from_rgb(0x38, 0xbd, 0xf8);
    let p_col = Color32::from_rgb(0x60, 0xa5, 0xfa);
    let e_col = Color32::from_rgb(0x34, 0xd3, 0x99);

    egui::ScrollArea::vertical().show(ui, |ui| {
        // 首屏上区：总使用率大圆环 + P/E 核圆环 + 极值
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(10.0, 8.0);
            egui::Frame::none()
                .fill(Color32::from_rgb(0x0b, 0x0f, 0x17))
                .stroke(Stroke::new(1.0_f32, COL_BORDER))
                .rounding(8.0)
                .inner_margin(egui::Margin::symmetric(12.0, 9.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        draw_ring_gauge(
                            ui,
                            64.0,
                            6.0,
                            cpu.total_usage / 100.0,
                            accent,
                            &format!("{:.0}%", cpu.total_usage),
                            14.5,
                        );
                        ui.add_space(10.0);
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(&cpu.brand)
                                    .color(Color32::WHITE)
                                    .strong()
                                    .size(13.0),
                            );
                            let temp_str = cpu
                                .temp_c
                                .map(|t| format!("{:.1}°C", t))
                                .unwrap_or_else(|| "—".to_string());
                            let tcol = if cpu.temp_c.is_some_and(|t| t >= 85.0) {
                                COL_BAD
                            } else if cpu.temp_c.is_some_and(|t| t >= 75.0) {
                                COL_WARN
                            } else {
                                COL_FG
                            };
                            ui.label(
                                RichText::new(if matches!(lang, UiLang::Zh) {
                                    format!(
                                        "温度 {}  ·  物理 {} 核 / 逻辑 {} 线程",
                                        temp_str, cpu.physical_cores, cpu.logical_cores
                                    )
                                } else {
                                    format!(
                                        "Temp {}  ·  {}C / {}T",
                                        temp_str, cpu.physical_cores, cpu.logical_cores
                                    )
                                })
                                .color(tcol)
                                .size(12.0),
                            );
                            let stat_str = stats
                                .cpu_usage
                                .as_ref()
                                .map(|s| {
                                    if matches!(lang, UiLang::Zh) {
                                        format!("  ·  占用均{:.0}% 峰{:.0}%", s.avg, s.max)
                                    } else {
                                        format!("  ·  Avg {:.0}% Peak {:.0}%", s.avg, s.max)
                                    }
                                })
                                .unwrap_or_default();
                            ui.label(
                                RichText::new(if matches!(lang, UiLang::Zh) {
                                    format!(
                                        "均频 {} MHz  ·  峰值 {} MHz{}",
                                        cpu.avg_freq_mhz, cpu.max_freq_mhz, stat_str
                                    )
                                } else {
                                    format!(
                                        "Avg {} MHz  ·  Max {} MHz{}",
                                        cpu.avg_freq_mhz, cpu.max_freq_mhz, stat_str
                                    )
                                })
                                .color(COL_DIM)
                                .size(11.5),
                            );
                        });
                    });
                });

            if cpu.has_hybrid {
                egui::Frame::none()
                    .fill(Color32::from_rgb(0x0b, 0x0f, 0x17))
                    .stroke(Stroke::new(1.0_f32, COL_BORDER))
                    .rounding(8.0)
                    .inner_margin(egui::Margin::symmetric(10.0, 9.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            draw_ring_gauge(
                                ui,
                                56.0,
                                5.5,
                                cpu.p_usage / 100.0,
                                p_col,
                                &format!("{:.0}%", cpu.p_usage),
                                12.5,
                            );
                            ui.add_space(8.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(if matches!(lang, UiLang::Zh) {
                                        format!("P 性能核 ×{}", cpu.p_count)
                                    } else {
                                        format!("P-Core ×{}", cpu.p_count)
                                    })
                                        .color(p_col)
                                        .strong()
                                        .size(12.0),
                                );
                                ui.label(
                                    RichText::new(format!("{} MHz", cpu.p_freq_mhz))
                                        .color(COL_FG)
                                        .size(11.5),
                                );
                            });
                        });
                    });

                egui::Frame::none()
                    .fill(Color32::from_rgb(0x0b, 0x0f, 0x17))
                    .stroke(Stroke::new(1.0_f32, COL_BORDER))
                    .rounding(8.0)
                    .inner_margin(egui::Margin::symmetric(10.0, 9.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            draw_ring_gauge(
                                ui,
                                56.0,
                                5.5,
                                cpu.e_usage / 100.0,
                                e_col,
                                &format!("{:.0}%", cpu.e_usage),
                                12.5,
                            );
                            ui.add_space(8.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(if matches!(lang, UiLang::Zh) {
                                        format!("E 能效核 ×{}", cpu.e_count)
                                    } else {
                                        format!("E-Core ×{}", cpu.e_count)
                                    })
                                        .color(e_col)
                                        .strong()
                                        .size(12.0),
                                );
                                ui.label(
                                    RichText::new(format!("{} MHz", cpu.e_freq_mhz))
                                        .color(COL_FG)
                                        .size(11.5),
                                );
                            });
                        });
                    });
            }
        });

        // 首屏中区：60 秒实时趋势与异常点标注（优先于逐核网格露出）
        if !cpu_points.is_empty() {
            ui.add_space(10.0);
            egui::Frame::none()
                .fill(Color32::from_rgb(0x0b, 0x0f, 0x17))
                .stroke(Stroke::new(1.0_f32, COL_BORDER))
                .rounding(8.0)
                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(tr(lang, "CPU 60 秒实时趋势与异常点标注", "CPU 60s Live Trend & Alerts"))
                                .color(Color32::WHITE)
                                .strong()
                                .size(12.5),
                        );
                        ui.label(
                            RichText::new(tr(
                                lang,
                                "（蓝线：占用 % · 橙线：温度 °C · 红点：异常/降频时刻）",
                                "(Blue: Usage % · Orange: Temp °C · Red: Alert/Throttle)",
                            ))
                            .color(COL_DIM)
                            .size(11.0),
                        );
                    });
                    ui.add_space(4.0);
                    let usage_pts: Vec<[f64; 2]> = cpu_points
                        .iter()
                        .enumerate()
                        .map(|(i, p)| [i as f64, f64::from(p.total_usage)])
                        .collect();
                    let temp_pts: Vec<[f64; 2]> = cpu_points
                        .iter()
                        .enumerate()
                        .filter_map(|(i, p)| p.temp_c.map(|t| [i as f64, t]))
                        .collect();
                    let alert_pts: Vec<[f64; 2]> = cpu_points
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.alert)
                        .map(|(i, p)| [i as f64, f64::from(p.total_usage)])
                        .collect();

                    egui_plot::Plot::new("cpu_live_trend_plot")
                        .height(135.0)
                        .include_y(0.0)
                        .include_y(100.0)
                        .allow_zoom(false)
                        .allow_drag(false)
                        .show_axes([false, true])
                        .show(ui, |plot_ui| {
                            plot_ui.line(
                                egui_plot::Line::new(egui_plot::PlotPoints::new(usage_pts))
                                    .name(tr(lang, "CPU 占用 %", "CPU Usage %"))
                                    .color(accent)
                                    .width(2.0_f32),
                            );
                            if !temp_pts.is_empty() {
                                plot_ui.line(
                                    egui_plot::Line::new(egui_plot::PlotPoints::new(temp_pts))
                                        .name(tr(lang, "CPU 温度 °C", "CPU Temp °C"))
                                        .color(COL_WARN)
                                        .width(1.5_f32),
                                );
                            }
                            if !alert_pts.is_empty() {
                                plot_ui.points(
                                    egui_plot::Points::new(egui_plot::PlotPoints::new(alert_pts))
                                        .name(tr(lang, "异常/降频", "Alert/Throttle"))
                                        .color(COL_BAD)
                                        .radius(4.5_f32),
                                );
                            }
                        });
                });
        }

        // 首屏下区：Top 5 CPU / 内存占用进程榜
        if !cpu.top_cpu_procs.is_empty() || !cpu.top_mem_procs.is_empty() {
            ui.add_space(10.0);
            ui.columns(2, |cols_ui| {
                egui::Frame::none()
                    .fill(Color32::from_rgb(0x0b, 0x0f, 0x17))
                    .stroke(Stroke::new(1.0_f32, COL_BORDER))
                    .rounding(8.0)
                    .inner_margin(egui::Margin::symmetric(12.0, 9.0))
                    .show(&mut cols_ui[0], |ui| {
                        ui.label(RichText::new(tr(lang, "Top 5 CPU 占用进程", "Top 5 CPU Processes")).color(accent).strong().size(12.5));
                        ui.add_space(4.0);
                        for p in &cpu.top_cpu_procs {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("{} (PID {})", p.name, p.pid)).color(COL_FG).size(11.5));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(format!("{:.1}% · {} MB", p.cpu_pct, p.mem_mb)).color(accent).size(11.5));
                                });
                            });
                        }
                    });
                egui::Frame::none()
                    .fill(Color32::from_rgb(0x0b, 0x0f, 0x17))
                    .stroke(Stroke::new(1.0_f32, COL_BORDER))
                    .rounding(8.0)
                    .inner_margin(egui::Margin::symmetric(12.0, 9.0))
                    .show(&mut cols_ui[1], |ui| {
                        ui.label(RichText::new(tr(lang, "Top 5 内存占用进程", "Top 5 Memory Processes")).color(e_col).strong().size(12.5));
                        ui.add_space(4.0);
                        for p in &cpu.top_mem_procs {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("{} (PID {})", p.name, p.pid)).color(COL_FG).size(11.5));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(format!("{} MB · {:.1}%", p.mem_mb, p.cpu_pct)).color(e_col).size(11.5));
                                });
                            });
                        }
                    });
            });
        }

        // 折叠式逐核圆环网格
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let toggle_txt = if *cores_expanded {
                if matches!(lang, UiLang::Zh) {
                    format!("▼ 逐逻辑核心环形负载（{} 核，点击收起）", cpu.cores.len())
                } else {
                    format!("▼ Per-Core Ring Load ({} threads, click to collapse)", cpu.cores.len())
                }
            } else if matches!(lang, UiLang::Zh) {
                format!("▶ 逐逻辑核心环形负载（{} 核，点击展开）", cpu.cores.len())
            } else {
                format!("▶ Per-Core Ring Load ({} threads, click to expand)", cpu.cores.len())
            };
            if ui.button(RichText::new(toggle_txt).size(12.0).color(COL_FG)).clicked() {
                *cores_expanded = !*cores_expanded;
            }
        });
        if *cores_expanded {
            ui.add_space(6.0);
            let avail = ui.available_width();
            let cols = ((avail / 122.0).floor() as usize).clamp(2, 10);
            let cell_w = ((avail - (cols.saturating_sub(1) as f32) * 8.0) / cols as f32).max(96.0);

            egui::Grid::new("cpu_tab_cores_ring_grid")
                .spacing(egui::vec2(8.0, 8.0))
                .show(ui, |ui| {
                    for (idx, c) in cpu.cores.iter().enumerate() {
                        let kcol = match c.kind {
                            "P" => p_col,
                            "E" => e_col,
                            _ => accent,
                        };
                        egui::Frame::none()
                            .fill(Color32::from_rgb(0x0b, 0x0f, 0x17))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x20, 0x29, 0x38)))
                            .rounding(6.0)
                            .inner_margin(egui::Margin::symmetric(8.0, 7.0))
                            .show(ui, |ui| {
                                ui.set_width((cell_w - 18.0).max(80.0));
                                ui.vertical_centered(|ui| {
                                    draw_ring_gauge(
                                        ui,
                                        44.0,
                                        4.5,
                                        c.usage / 100.0,
                                        kcol,
                                        &format!("{:.0}%", c.usage),
                                        10.5,
                                    );
                                    ui.add_space(3.0);
                                    ui.label(
                                        RichText::new(format!("Core {} · {}", c.id, c.kind))
                                            .color(kcol)
                                            .strong()
                                            .size(10.5),
                                    );
                                    ui.label(
                                        RichText::new(format!("{} MHz", c.freq_mhz))
                                            .color(COL_DIM)
                                            .size(10.0),
                                    );
                                });
                            });
                        if (idx + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }
                });
        }
    });
}
