//! 内置网页服务端模块：通过 axum 暴露完整状态与控制接口，并内嵌 Vue 3 前端静态资源。

use crate::collectors::GpuSampler;
use crate::daemon::{request_stop_bg, start_supervisor_bg};
use crate::infra::{
    data_dir, delete_session_data, export_session_zip, incidents_dir, install_startup_reg,
    is_startup_installed, latest_session_path, load_config, open_folder_in_explorer, running_pid,
    save_config, sessions_dir, uninstall_startup_reg, Config,
};
use axum::extract::{Query, Request, State};
use axum::http::{header, Method, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Local};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::net::{IpAddr, Ipv6Addr, UdpSocket};
use std::path::{Path, PathBuf};
use std::thread;
use tokio::sync::watch;

#[derive(RustEmbed)]
#[folder = "web/dist"]
struct WebAssets;

pub const WEB_PORT: u16 = 8787;
pub const DISCOVERY_UDP_PORT: u16 = 8788;

/// 网页服务句柄，持有停机广播发送端与服务线程句柄
pub struct WebServerHandle {
    shutdown_tx: Option<watch::Sender<bool>>,
    join_handle: Option<thread::JoinHandle<()>>,
    pub lan_url: String,
    pub local_url: String,
    pub local_ipv6_url: String,
    pub ipv6_url: Option<String>,
}

impl WebServerHandle {
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }
        if let Some(jh) = self.join_handle.take() {
            let _ = jh.join();
        }
    }
}

impl Drop for WebServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 获取本机局域网 IPv4 地址（仅查询系统路由表，不发送网络数据包）
pub fn detect_lan_ip() -> Option<String> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    let addr = sock.local_addr().ok()?;
    let ip = addr.ip();
    if ip.is_ipv4() && !ip.is_loopback() {
        Some(ip.to_string())
    } else {
        None
    }
}

/// 判断 IPv6 是否为非本地链路、非 ULA 内网的公网单播地址
fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    let seg0 = ip.segments()[0];
    !ip.is_loopback()
        && !ip.is_unspecified()
        && !ip.is_multicast()
        && (seg0 & 0xffc0) != 0xfe80 // 非链路本地 fe80::/10
        && (seg0 & 0xfe00) != 0xfc00 // 非唯一本地 fc00::/7
}

/// 获取本机外网/公网 IPv6 地址（仅查询系统路由表，不发送网络数据包）
pub fn detect_public_ipv6() -> Option<String> {
    let sock = UdpSocket::bind("[::]:0").ok()?;
    sock.connect("[2001:4860:4860::8888]:80").ok()?;
    let addr = sock.local_addr().ok()?;
    if let IpAddr::V6(v6) = addr.ip() {
        if is_public_ipv6(v6) {
            return Some(v6.to_string());
        }
    }
    None
}

/// 在后台线程同时绑定 IPv4 (0.0.0.0:port) 与 IPv6 ([::]:port，涵盖本机 ::1 与全部外网 IPv6)
pub fn start_web_server(port: u16) -> Result<WebServerHandle, String> {
    let bind_v4 = format!("0.0.0.0:{}", port);
    let std_v4 = std::net::TcpListener::bind(&bind_v4).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            format!(
                "端口 {} 已被占用（请检查是否已有进程占用，或在「设置」中更换其他端口）",
                port
            )
        } else {
            format!("端口 {} 启动失败：{}", port, e)
        }
    })?;
    std_v4
        .set_nonblocking(true)
        .map_err(|e| format!("设置非阻塞失败：{}", e))?;

    let bind_v6 = format!("[::]:{}", port);
    let std_v6 = match std::net::TcpListener::bind(&bind_v6) {
        Ok(l) => {
            if let Err(e) = l.set_nonblocking(true) {
                crate::infra::boot_log(&format!("[warn] IPv6 set_nonblocking failed: {}", e));
                None
            } else {
                crate::infra::boot_log(&format!("[info] web server bound IPv4 {} and IPv6 {}", bind_v4, bind_v6));
                Some(l)
            }
        }
        Err(e) => {
            crate::infra::boot_log(&format!("[warn] IPv6 bind {} failed (IPv4 still active): {}", bind_v6, e));
            None
        }
    };

    let (tx, rx) = watch::channel(false);
    let rx_udp = rx.clone();
    thread::spawn(move || run_udp_discovery_responder(port, rx_udp));

    let join_handle = thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        else {
            return;
        };
        rt.block_on(async move {
            let Ok(v4_listener) = tokio::net::TcpListener::from_std(std_v4) else {
                return;
            };
            let v6_listener = std_v6.and_then(|l| tokio::net::TcpListener::from_std(l).ok());
            let app = Router::new()
                .route("/api/auth/status", get(api_auth_status))
                .route("/api/auth/verify", post(api_auth_verify))
                .route("/api/state", get(api_state))
                .route("/api/collector/start", post(api_collector_start))
                .route("/api/collector/stop", post(api_collector_stop))
                .route("/api/live/start", post(api_live_start))
                .route("/api/live/stop", post(api_live_stop))
                .route("/api/live/summary", get(api_live_summary))
                .route("/api/live/save", post(api_live_save))
                .route("/api/startup", post(api_startup))
                .route("/api/config", get(api_get_config).post(api_save_config))
                .route("/api/sessions/delete", post(api_delete_sessions))
                .route("/api/sessions/export", get(api_export_session))
                .route("/api/incidents", get(api_incidents))
                .route("/api/incidents/preview", get(api_incident_preview))
                .route("/api/open-folder", post(api_open_folder))
                .layer(middleware::from_fn(auth_guard))
                .fallback(static_handler)
                .with_state(port);

            let mut rx_v4 = rx.clone();
            let app_v4 = app.clone();
            let srv_v4 = axum::serve(v4_listener, app_v4).with_graceful_shutdown(async move {
                while !*rx_v4.borrow() {
                    if rx_v4.changed().await.is_err() {
                        break;
                    }
                }
            });

            let mut rx_stop = rx.clone();
            let serve_fut = async move {
                if let Some(v6_l) = v6_listener {
                    let mut rx_v6 = rx.clone();
                    let srv_v6 = axum::serve(v6_l, app).with_graceful_shutdown(async move {
                        while !*rx_v6.borrow() {
                            if rx_v6.changed().await.is_err() {
                                break;
                            }
                        }
                    });
                    let _ = tokio::join!(srv_v4, srv_v6);
                } else {
                    let _ = srv_v4.await;
                }
            };

            // 收到停机信号后最多等待 250ms 优雅关闭 Keep-Alive 连接，超时立即释放监听端口
            tokio::select! {
                _ = serve_fut => {}
                _ = async {
                    while !*rx_stop.borrow() {
                        if rx_stop.changed().await.is_err() {
                            break;
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                } => {}
            }
        });
    });

    let local_url = format!("http://127.0.0.1:{}", port);
    let lan_url = detect_lan_ip()
        .map(|ip| format!("http://{}:{}", ip, port))
        .unwrap_or_else(|| local_url.clone());
    let local_ipv6_url = format!("http://[::1]:{}", port);
    let ipv6_url = detect_public_ipv6().map(|ip| format!("http://[{}]:{}", ip, port));

    Ok(WebServerHandle {
        shutdown_tx: Some(tx),
        join_handle: Some(join_handle),
        lan_url,
        local_url,
        local_ipv6_url,
        ipv6_url,
    })
}

/// 静态资源托管（支持单页应用回退到 index.html）
async fn static_handler(uri: Uri) -> Response {
    let raw = uri.path().trim_start_matches('/');
    let path = if raw.is_empty() { "index.html" } else { raw };

    if let Some(file) = WebAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            [(header::CONTENT_TYPE, mime.as_ref())],
            file.data.into_owned(),
        )
            .into_response();
    }
    if let Some(index) = WebAssets::get("index.html") {
        return (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            index.data.into_owned(),
        )
            .into_response();
    }
    (StatusCode::NOT_FOUND, "Not Found").into_response()
}

// --------------------------------------------------------------------------- REST API

#[derive(Deserialize)]
struct StateQuery {
    session: Option<String>,
    full: Option<String>,
}

#[derive(Serialize)]
struct SessionMeta {
    name: String,
    size_label: String,
    active: bool,
}

#[derive(Serialize)]
struct GpuRowDto {
    ts: String,
    temp: String,
    hotspot: String,
    power: String,
    core: String,
    memclk: String,
    pcie: String,
    util: String,
    vram: String,
    fan: String,
    thr: String,
    thr_label: String,
    ok: String,
    sev: &'static str,
}

#[derive(Serialize)]
struct MemPointDto {
    ts: String,
    commit_pct: f64,
    nonpaged_mb: f64,
}

#[derive(Serialize)]
struct LogEntryDto {
    time: String,
    level: String,
    source: String,
    message: String,
    raw: String,
    sev: &'static str,
}

async fn api_state(State(port): State<u16>, Query(q): Query<StateQuery>) -> Json<Value> {
    let collector_running = running_pid().is_some();
    let live_running = crate::live::is_running();
    let live_has_data = crate::live::has_data();
    let active_path = if collector_running {
        latest_session_path()
    } else {
        None
    };

    let mut session_dirs = list_session_dirs();
    session_dirs.sort_by(|a, b| b.cmp(a));

    let mut sessions: Vec<SessionMeta> = session_dirs
        .iter()
        .map(|p| {
            let name = p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            SessionMeta {
                name,
                size_label: dir_size_label(p),
                active: active_path.as_ref() == Some(p),
            }
        })
        .collect();

    if live_running || live_has_data {
        sessions.insert(
            0,
            SessionMeta {
                name: crate::live::LIVE_SESSION_NAME.to_string(),
                size_label: "仅内存".to_string(),
                active: live_running,
            },
        );
    }

    let req_sess = q.session.as_deref().unwrap_or("");
    let use_live = req_sess == crate::live::LIVE_SESSION_NAME
        || (!collector_running && req_sess.is_empty() && (live_running || live_has_data));

    if use_live {
        let snap = crate::live::snapshot();
        let hb = if snap.running {
            json!({ "text": "● 实时监测中（仅内存日志）", "level": "ok" })
        } else {
            json!({ "text": "● 实时监测已停止（保留内存日志）", "level": "warn" })
        };
        let lan_url = detect_lan_ip()
            .map(|ip| format!("http://{}:{}", ip, port))
            .unwrap_or_else(|| format!("http://127.0.0.1:{}", port));
        let local_ipv6_url = format!("http://[::1]:{}", port);
        let ipv6_url = detect_public_ipv6()
            .map(|ip| format!("http://[{}]:{}", ip, port))
            .unwrap_or_default();

        return Json(json!({
            "collector_running": collector_running,
            "live_running": live_running,
            "live_has_data": live_has_data,
            "heartbeat": hb,
            "session": crate::live::LIVE_SESSION_NAME,
            "sessions": sessions,
            "gpu_summary": snap.gpu_summary,
            "mem_summary": snap.mem_summary,
            "proc_summary": snap.proc_summary,
            "gpu_rows": snap.gpu_rows,
            "cpu_points": snap.cpu_points,
            "mem_points": snap.mem_points,
            "events": snap.events,
            "wer": snap.wer,
            "alerts": snap.alerts,
            "alert_count": snap.alert_count,
            "cpu": snap.cpu,
            "stats": snap.stats,
            "live_started_at": snap.started_at,
            "read_only_remote": load_config().read_only_remote,
            "startup_enabled": is_startup_installed(),
            "full_history": true,
            "lan_url": lan_url,
            "local_ipv6_url": local_ipv6_url,
            "ipv6_url": ipv6_url,
        }));
    }

    let picked_dir = q
        .session
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|name| sessions_dir().join(name))
        .filter(|p| p.is_dir())
        .or_else(|| active_path.clone().filter(|p| p.is_dir()))
        .or_else(|| session_dirs.first().cloned());

    let full = q.full.as_deref() == Some("1");
    let max_bytes = if full { u64::MAX } else { 128 * 1024 };

    let mut gpu_rows = Vec::new();
    let mut gpu_summary = "—".to_string();
    let mut mem_points = Vec::new();
    let mut mem_summary = "—".to_string();
    let mut proc_summary = "—".to_string();
    let mut events = Vec::new();
    let mut wer = Vec::new();
    let mut alerts = Vec::new();
    let mut alert_count = 0usize;
    let mut hb = json!({ "text": "● 未在采集（暂无会话）", "level": "dim" });
    let mut session_name = String::new();

    if let Some(ref dir) = picked_dir {
        session_name = dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        // 心跳判断
        if !collector_running {
            hb = json!({ "text": "● 未在采集（查看历史）", "level": "dim" });
        } else if let Ok(s) = fs::read_to_string(dir.join("heartbeat.txt")) {
            if let Ok(dt) = DateTime::parse_from_rfc3339(s.trim()) {
                let age = Local::now()
                    .signed_duration_since(dt.with_timezone(&Local))
                    .num_seconds();
                if age < 5 {
                    hb = json!({ "text": "● 采集器运行中", "level": "ok" });
                } else {
                    hb = json!({ "text": format!("● 心跳延迟 {}s", age), "level": "warn" });
                }
            }
        } else {
            hb = json!({ "text": "● 采集器初始化中...", "level": "warn" });
        }

        // GPU
        let gpu_lines = read_tail_lines(&dir.join("gpu.csv"), max_bytes);
        let gpu_limit = if full { 10000 } else { 600 };
        for ln in gpu_lines {
            if ln.starts_with("ts,") {
                continue;
            }
            let p: Vec<&str> = ln.split(',').collect();
            if p.len() < 16 {
                continue;
            }
            let (ts_raw, temp, power, core, memc, gen, wid, util, vram, fan, thr, ok) = (
                p[0], p[1], p[2], p[4], p[5], p[6], p[7], p[8], p[10], p[12], p[13], p[14],
            );
            let hotspot = p.get(17).map(|s| s.trim()).unwrap_or("");
            let bad = ok.trim() != "OK";
            let temp_f = temp.trim().parse::<f64>().ok();
            let hotspot_f = hotspot.parse::<f64>().ok();
            let power_f = power.trim().parse::<f64>().ok();
            let gen_i = gen.trim().parse::<i64>().ok();
            let warn = temp_f.is_some_and(|t| t >= 75.0)
                || hotspot_f.is_some_and(|h| h >= 92.0)
                || match (temp_f, hotspot_f) {
                    (Some(t), Some(h)) => (h - t) >= 20.0,
                    _ => false,
                }
                || power_f.is_some_and(|w| w >= 350.0)
                || (power_f.is_some_and(|w| w > 150.0) && gen_i.is_some_and(|g| g < 4));
            let sev = if bad {
                "bad"
            } else if warn {
                "warn"
            } else {
                "normal"
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
                u64::from_str_radix(hex, 16)
                    .map(|v| format!("0x{:x}", v))
                    .unwrap_or_else(|_| thr.chars().take(10).collect())
            };
            let thr_label = crate::infra::decode_throttle_label(&thr_short);
            let ts_short = if ts_raw.len() > 19 {
                ts_raw[11..19].to_string()
            } else {
                ts_raw.to_string()
            };
            gpu_summary = format!(
                "{}°C  {}W  {}MHz  {}  util {}%  VRAM {}MB",
                temp, power, core, pcie, util, vram
            );
            gpu_rows.push(GpuRowDto {
                ts: ts_short,
                temp: temp.to_string(),
                hotspot: hotspot.to_string(),
                power: power.to_string(),
                core: core.to_string(),
                memclk: memc.to_string(),
                pcie,
                util: util.to_string(),
                vram: vram.to_string(),
                fan: fan.to_string(),
                thr: thr_short,
                thr_label,
                ok: ok.to_string(),
                sev,
            });
        }
        if gpu_rows.len() > gpu_limit {
            gpu_rows.drain(0..(gpu_rows.len() - gpu_limit));
        }

        // Mem
        for ln in read_tail_lines(&dir.join("mem.csv"), max_bytes) {
            if ln.starts_with("ts,") {
                continue;
            }
            let p: Vec<&str> = ln.split(',').collect();
            if p.len() < 8 {
                continue;
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
            mem_summary = format!("{:.1}/{:.1}GB ({:.0}%)", commit, limit, pct);
            mem_points.push(MemPointDto {
                ts: ts_short,
                commit_pct: pct,
                nonpaged_mb: nonpaged,
            });
        }
        if mem_points.len() > 300 {
            mem_points.drain(0..(mem_points.len() - 300));
        }

        // Proc
        for ln in read_tail_lines(&dir.join("proc.csv"), 16 * 1024) {
            if ln.starts_with("ts,") {
                continue;
            }
            let p: Vec<&str> = ln.split(',').collect();
            if p.len() < 5 {
                continue;
            }
            proc_summary = if p[1].trim() == "-" {
                "未运行".to_string()
            } else {
                format!("{} ({}MB)", p[1].trim(), p[3].trim())
            };
        }

        // Events
        for ln in read_tail_lines(&dir.join("events.csv"), max_bytes) {
            if ln.starts_with("ts,") {
                continue;
            }
            let p: Vec<&str> = ln.splitn(7, ',').collect();
            if p.len() < 7 {
                continue;
            }
            let (ts, lg, lvl, eid, prov, _rid, msg) = (p[0], p[1], p[2], p[3], p[4], p[5], p[6]);
            let is_bad = matches!(lvl, "Error" | "Critical")
                || matches!(eid, "41" | "4101" | "4102" | "1001" | "117" | "219");
            let sev = if is_bad {
                "bad"
            } else if lvl == "Warning" {
                "warn"
            } else {
                "normal"
            };
            events.push(LogEntryDto {
                time: if ts.len() >= 19 { &ts[11..19] } else { ts }.to_string(),
                level: if lvl.is_empty() { "Info" } else { lvl }.to_string(),
                source: format!("{} · {} #{}", lg, prov, eid),
                message: msg.trim_matches('"').to_string(),
                raw: ln,
                sev,
            });
        }
        if events.len() > 1500 {
            events.drain(0..(events.len() - 1500));
        }

        // WER
        for ln in read_tail_lines(&dir.join("wer.csv"), max_bytes) {
            if ln.starts_with("ts,") {
                continue;
            }
            let p: Vec<&str> = ln.splitn(5, ',').collect();
            let up = ln.to_uppercase();
            let sev = if up.contains("KERNEL") || up.contains("LIVEDUMP") {
                "bad"
            } else {
                "warn"
            };
            let (time, level, source, message) = if p.len() >= 5 {
                (
                    if p[0].len() >= 19 {
                        &p[0][11..19]
                    } else {
                        p[0]
                    }
                    .to_string(),
                    p[1].to_string(),
                    p[3].to_string(),
                    format!("{} ({})", p[2], p[4]),
                )
            } else {
                (String::new(), "WER".to_string(), String::new(), ln.clone())
            };
            wer.push(LogEntryDto {
                time,
                level,
                source,
                message,
                raw: ln,
                sev,
            });
        }

        // Alerts
        for ln in read_tail_lines(&dir.join("alerts.log"), max_bytes) {
            alert_count += 1;
            let bad = ["[INCIDENT]", "SMI_FAIL", "PCIE", "GAME_EXIT", "WER_NEW"]
                .iter()
                .any(|k| ln.contains(k));
            let sev = if bad { "bad" } else { "warn" };
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
                (String::new(), "ALERT".to_string(), ln.clone())
            };
            alerts.push(LogEntryDto {
                time,
                level,
                source: "告警引擎".to_string(),
                message,
                raw: ln,
                sev,
            });
        }
        if alerts.len() > 1500 {
            alerts.drain(0..(alerts.len() - 1500));
        }
    }

    let lan_url = detect_lan_ip()
        .map(|ip| format!("http://{}:{}", ip, port))
        .unwrap_or_else(|| format!("http://127.0.0.1:{}", port));
    let local_ipv6_url = format!("http://[::1]:{}", port);
    let ipv6_url = detect_public_ipv6()
        .map(|ip| format!("http://[{}]:{}", ip, port))
        .unwrap_or_default();

    let hist_stats = crate::live::LiveStatsSummary {
        gpu_temp: crate::live::calc_metric_stats(gpu_rows.iter().filter_map(|r| r.temp.trim().parse::<f64>().ok())),
        gpu_hotspot: crate::live::calc_metric_stats(gpu_rows.iter().filter_map(|r| r.hotspot.trim().parse::<f64>().ok())),
        gpu_power: crate::live::calc_metric_stats(gpu_rows.iter().filter_map(|r| r.power.trim().parse::<f64>().ok())),
        gpu_core: crate::live::calc_metric_stats(gpu_rows.iter().filter_map(|r| r.core.trim().parse::<f64>().ok())),
        cpu_usage: None,
        cpu_temp: None,
    };

    Json(json!({
        "collector_running": collector_running,
        "live_running": live_running,
        "live_has_data": live_has_data,
        "heartbeat": hb,
        "session": session_name,
        "sessions": sessions,
        "gpu_summary": gpu_summary,
        "mem_summary": mem_summary,
        "proc_summary": proc_summary,
        "gpu_rows": gpu_rows,
        "cpu_points": Value::Array(Vec::new()),
        "mem_points": mem_points,
        "events": events,
        "wer": wer,
        "alerts": alerts,
        "alert_count": alert_count,
        "cpu": Value::Null,
        "stats": hist_stats,
        "read_only_remote": load_config().read_only_remote,
        "startup_enabled": is_startup_installed(),
        "full_history": full,
        "lan_url": lan_url,
        "local_ipv6_url": local_ipv6_url,
        "ipv6_url": ipv6_url,
    }))
}

async fn api_auth_status() -> Json<Value> {
    let cfg = load_config();
    Json(json!({
        "auth_required": !cfg.auth_pin.trim().is_empty(),
        "read_only_remote": cfg.read_only_remote,
    }))
}

#[derive(Deserialize)]
struct AuthVerifyReq {
    pin: String,
}

async fn api_auth_verify(Json(req): Json<AuthVerifyReq>) -> Response {
    let cfg = load_config();
    let expected = cfg.auth_pin.trim();
    if expected.is_empty() || req.pin.trim() == expected {
        (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
    } else {
        (StatusCode::UNAUTHORIZED, Json(json!({ "ok": false, "error": "口令错误" }))).into_response()
    }
}

async fn auth_guard(req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if !path.starts_with("/api/") || path.starts_with("/api/auth/") {
        return next.run(req).await;
    }
    let cfg = load_config();
    let expected_pin = cfg.auth_pin.trim();
    if !expected_pin.is_empty() {
        let header_pin = req
            .headers()
            .get("x-gpucap-pin")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let query_pin = req
            .uri()
            .query()
            .and_then(|q| {
                q.split('&')
                    .find_map(|pair| pair.strip_prefix("pin="))
            })
            .unwrap_or("");
        if header_pin != expected_pin && query_pin != expected_pin {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "需要访问口令", "auth_required": true })),
            )
                .into_response();
        }
    }
    if cfg.read_only_remote && req.method() == Method::POST {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "当前已开启只读访客模式，禁止远程控制" })),
        )
            .into_response();
    }
    next.run(req).await
}

async fn api_collector_start() -> Response {
    crate::live::stop_live();
    match start_supervisor_bg() {
        Ok(_) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn api_collector_stop() -> Json<Value> {
    request_stop_bg();
    Json(json!({ "ok": true }))
}

async fn api_live_start() -> Response {
    match crate::live::start_live() {
        Ok(_) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn api_live_stop() -> Json<Value> {
    crate::live::stop_live();
    Json(json!({ "ok": true }))
}

async fn api_live_summary() -> Json<Value> {
    let summary = crate::live::build_diagnostic_summary();
    Json(json!({ "ok": true, "summary": summary }))
}

async fn api_live_save() -> Response {
    match crate::live::save_snapshot_to_session() {
        Ok(session) => (StatusCode::OK, Json(json!({ "ok": true, "session": session }))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
}

/// 局域网 UDP 广播发现应答器（监听 8788 端口，响应 GPUCAP_DISCOVER 查询）。
fn run_udp_discovery_responder(web_port: u16, rx_stop: watch::Receiver<bool>) {
    let Ok(sock) = UdpSocket::bind(("0.0.0.0", DISCOVERY_UDP_PORT)) else {
        return;
    };
    let _ = sock.set_read_timeout(Some(std::time::Duration::from_millis(400)));
    let mut buf = [0u8; 256];
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "GPUCap-PC".to_string());

    while !*rx_stop.borrow() {
        if let Ok((n, peer)) = sock.recv_from(&mut buf) {
            let msg = String::from_utf8_lossy(&buf[..n]);
            if msg.contains("GPUCAP_DISCOVER") {
                let lan_url = detect_lan_ip()
                    .map(|ip| format!("http://{}:{}", ip, web_port))
                    .unwrap_or_else(|| format!("http://127.0.0.1:{}", web_port));
                let payload = json!({
                    "service": "gpucap",
                    "host": host,
                    "port": web_port,
                    "lan_url": lan_url,
                })
                .to_string();
                let _ = sock.send_to(payload.as_bytes(), peer);
            }
        }
    }
}

#[derive(Deserialize)]
struct StartupReq {
    enabled: bool,
}

async fn api_startup(Json(req): Json<StartupReq>) -> Response {
    let res = if req.enabled {
        install_startup_reg()
    } else {
        uninstall_startup_reg()
    };
    match res {
        Ok(_) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn api_get_config() -> Json<Value> {
    let cfg = load_config();
    let gpus: Vec<Value> = GpuSampler::list_gpus()
        .into_iter()
        .map(|(i, n)| json!({ "index": i, "name": n }))
        .collect();
    Json(json!({ "config": cfg, "gpus": gpus }))
}

async fn api_save_config(Json(cfg): Json<Config>) -> Response {
    match save_config(&cfg) {
        Ok(_) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct DeleteReq {
    names: Vec<String>,
}

async fn api_delete_sessions(Json(req): Json<DeleteReq>) -> Json<Value> {
    let active = if running_pid().is_some() {
        latest_session_path()
    } else {
        None
    };
    let mut deleted_sessions = 0usize;
    let mut deleted_incidents = 0usize;
    for name in req.names {
        let dir = sessions_dir().join(&name);
        if active.as_ref() == Some(&dir) {
            continue;
        }
        if let Ok(n) = delete_session_data(&dir) {
            deleted_sessions += 1;
            deleted_incidents += n;
        }
    }
    Json(json!({
        "deleted_sessions": deleted_sessions,
        "deleted_incidents": deleted_incidents
    }))
}

#[derive(Deserialize)]
struct ExportQuery {
    session: String,
}

async fn api_export_session(Query(q): Query<ExportQuery>) -> Response {
    let dir = sessions_dir().join(&q.session);
    if !dir.is_dir() {
        return (StatusCode::NOT_FOUND, "会话不存在").into_response();
    }
    match export_session_zip(&dir) {
        Ok(zip_path) => match fs::read(&zip_path) {
            Ok(bytes) => {
                let fname = zip_path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "export.zip".to_string());
                let disp = format!("attachment; filename=\"{}\"", fname);
                (
                    [
                        (header::CONTENT_TYPE, "application/zip".to_string()),
                        (header::CONTENT_DISPOSITION, disp),
                    ],
                    bytes,
                )
                    .into_response()
            }
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

#[derive(Deserialize)]
struct IncidentListQuery {
    session: Option<String>,
}

async fn api_incidents(Query(q): Query<IncidentListQuery>) -> Json<Value> {
    let session_key = q
        .session
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| sessions_dir().join(s).to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let mut items = Vec::new();
    if let Ok(entries) = fs::read_dir(incidents_dir()) {
        for e in entries.flatten() {
            let dir = e.path();
            if !dir.is_dir() {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let mut reason = String::new();
            let mut belongs = false;
            if let Ok(raw) = fs::read_to_string(dir.join("incident.json")) {
                if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                    reason = v
                        .get("reason")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    let owner = v.get("session").and_then(|s| s.as_str()).unwrap_or("");
                    belongs = !session_key.is_empty() && owner.to_lowercase() == session_key;
                }
            }
            items.push(json!({
                "name": name,
                "reason": reason,
                "belongs_to_session": belongs,
            }));
        }
    }
    items.sort_by(|a, b| {
        let na = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let nb = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
        nb.cmp(na)
    });
    Json(json!({ "items": items }))
}

#[derive(Deserialize)]
struct IncidentPreviewQuery {
    name: String,
    file: String,
}

async fn api_incident_preview(Query(q): Query<IncidentPreviewQuery>) -> Json<Value> {
    let dir = incidents_dir().join(&q.name);
    let target = if q.file == "Report_*.wer" {
        fs::read_dir(&dir).ok().and_then(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|x| x == "wer"))
        })
    } else {
        Some(dir.join(&q.file))
    };
    let Some(path) = target else {
        return Json(json!({ "text": "(该事故目录下暂无 Report.wer 文件)" }));
    };
    match fs::read(&path) {
        Ok(bytes) => {
            let text = if bytes.starts_with(&[0xFF, 0xFE]) {
                let u16s: Vec<u16> = bytes[2..]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                String::from_utf16_lossy(&u16s)
            } else {
                String::from_utf8_lossy(&bytes).into_owned()
            };
            let snippet = text.lines().take(200).collect::<Vec<_>>().join("\n");
            Json(json!({ "text": snippet }))
        }
        Err(_) => Json(json!({ "text": format!("(文件不存在或无权读取：{})", path.display()) })),
    }
}

#[derive(Deserialize)]
struct OpenFolderReq {
    kind: String,
    session: Option<String>,
}

async fn api_open_folder(Json(req): Json<OpenFolderReq>) -> Response {
    let target = match req.kind.as_str() {
        "session" => req
            .session
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| sessions_dir().join(s))
            .unwrap_or_else(data_dir),
        _ => data_dir(),
    };
    match open_folder_in_explorer(&target) {
        Ok(_) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        )
            .into_response(),
    }
}

// --------------------------------------------------------------------------- 工具函数

fn list_session_dirs() -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(sessions_dir()) else {
        return Vec::new();
    };
    rd.flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

fn dir_size_label(dir: &Path) -> String {
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

fn read_tail_lines(path: &Path, max_bytes: u64) -> Vec<String> {
    let Ok(meta) = fs::metadata(path) else {
        return Vec::new();
    };
    let len = meta.len();
    let Ok(mut f) = File::open(path) else {
        return Vec::new();
    };
    let mut skip_first = false;
    if len > max_bytes {
        let start = len - max_bytes;
        if f.seek(SeekFrom::Start(start)).is_err() {
            return Vec::new();
        }
        skip_first = true;
    }
    let mut reader = BufReader::with_capacity(16 * 1024, f);
    let mut out = Vec::new();
    let mut buf = String::with_capacity(256);
    loop {
        buf.clear();
        match reader.read_line(&mut buf) {
            Ok(0) => break,
            Ok(_) => {
                if skip_first {
                    skip_first = false;
                    continue;
                }
                let trimmed = buf.trim_end_matches(['\r', '\n']);
                if !trimmed.trim().is_empty() {
                    out.push(trimmed.to_string());
                }
            }
            Err(_) => break,
        }
    }
    out
}
