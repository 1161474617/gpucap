//! GPUCap 安卓端 Rust 核心引擎：
//! 使用 UniFFI 0.29 导出强类型接口给 Jetpack Compose，内部基于 Tokio + Reqwest(rustls) 处理
//! 并发测速、UDP 8788 局域网自动发现、PIN 鉴权与 REST 状态拉取。

use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum GpucapCoreError {
    #[error("需要主机访问口令")]
    AuthRequired,
    #[error("网络请求失败: {msg}")]
    Network { msg: String },
    #[error("服务端返回错误 ({code}): {msg}")]
    Http { code: u16, msg: String },
    #[error("数据解析失败: {msg}")]
    Decode { msg: String },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct HeartbeatInfo {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub level: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct SessionMeta {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size_label: String,
    #[serde(default)]
    pub active: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct GpuRow {
    #[serde(default)]
    pub ts: String,
    #[serde(default)]
    pub temp: String,
    #[serde(default)]
    pub hotspot: String,
    #[serde(default)]
    pub power: String,
    #[serde(default)]
    pub core: String,
    #[serde(default)]
    pub memclk: String,
    #[serde(default)]
    pub pcie: String,
    #[serde(default)]
    pub util: String,
    #[serde(default)]
    pub vram: String,
    #[serde(default)]
    pub vram_total: String,
    #[serde(default)]
    pub vram_pct: f64,
    #[serde(default)]
    pub pcie_drop: bool,
    #[serde(default)]
    pub fan: String,
    #[serde(default)]
    pub thr: String,
    #[serde(default)]
    pub thr_label: String,
    #[serde(default)]
    pub ok: String,
    #[serde(default)]
    pub sev: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct MemPoint {
    #[serde(default)]
    pub ts: String,
    #[serde(default)]
    pub commit_pct: f64,
    #[serde(default)]
    pub nonpaged_mb: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct LogEntry {
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub raw: String,
    #[serde(default)]
    pub sev: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct CpuCoreInfo {
    #[serde(default)]
    pub id: i32,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub usage: f64,
    #[serde(default)]
    pub freq_mhz: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct TopProcess {
    #[serde(default)]
    pub pid: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cpu_pct: f64,
    #[serde(default)]
    pub mem_mb: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct LiveCpuPoint {
    #[serde(default)]
    pub ts: String,
    #[serde(default)]
    pub total_usage: f64,
    #[serde(default)]
    pub avg_freq_mhz: i64,
    #[serde(default)]
    pub temp_c: Option<f64>,
    #[serde(default)]
    pub alert: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct CpuSnapshot {
    #[serde(default)]
    pub brand: String,
    #[serde(default)]
    pub temp_c: Option<f64>,
    #[serde(default)]
    pub total_usage: f64,
    #[serde(default)]
    pub avg_freq_mhz: i64,
    #[serde(default)]
    pub max_freq_mhz: i64,
    #[serde(default)]
    pub physical_cores: i32,
    #[serde(default)]
    pub logical_cores: i32,
    #[serde(default)]
    pub has_hybrid: bool,
    #[serde(default)]
    pub p_count: i32,
    #[serde(default)]
    pub p_usage: f64,
    #[serde(default)]
    pub p_freq_mhz: i64,
    #[serde(default)]
    pub e_count: i32,
    #[serde(default)]
    pub e_usage: f64,
    #[serde(default)]
    pub e_freq_mhz: i64,
    #[serde(default)]
    pub cores: Vec<CpuCoreInfo>,
    #[serde(default)]
    pub top_cpu_procs: Vec<TopProcess>,
    #[serde(default)]
    pub top_mem_procs: Vec<TopProcess>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct MetricStats {
    #[serde(default)]
    pub min: f64,
    #[serde(default)]
    pub avg: f64,
    #[serde(default)]
    pub max: f64,
    #[serde(default)]
    pub count: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct LiveStatsSummary {
    #[serde(default)]
    pub gpu_temp: Option<MetricStats>,
    #[serde(default)]
    pub gpu_hotspot: Option<MetricStats>,
    #[serde(default)]
    pub gpu_power: Option<MetricStats>,
    #[serde(default)]
    pub gpu_core: Option<MetricStats>,
    #[serde(default)]
    pub cpu_usage: Option<MetricStats>,
    #[serde(default)]
    pub cpu_temp: Option<MetricStats>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct EnvFingerprint {
    #[serde(default)]
    pub gpu_name: String,
    #[serde(default)]
    pub driver_version: String,
    #[serde(default)]
    pub vram_total_mb: i64,
    #[serde(default)]
    pub pcie_max: String,
    #[serde(default)]
    pub vbios: String,
    #[serde(default)]
    pub os_version: String,
    #[serde(default)]
    pub power_plan: String,
    #[serde(default)]
    pub driver_changed_from: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct StateSnapshot {
    #[serde(default)]
    pub collector_running: bool,
    #[serde(default)]
    pub live_running: bool,
    #[serde(default)]
    pub live_has_data: bool,
    #[serde(default)]
    pub cpu: Option<CpuSnapshot>,
    #[serde(default)]
    pub cpu_points: Vec<LiveCpuPoint>,
    #[serde(default)]
    pub stats: LiveStatsSummary,
    #[serde(default)]
    pub env: Option<EnvFingerprint>,
    #[serde(default)]
    pub read_only_remote: bool,
    #[serde(default)]
    pub heartbeat: HeartbeatInfo,
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub sessions: Vec<SessionMeta>,
    #[serde(default)]
    pub gpu_summary: String,
    #[serde(default)]
    pub mem_summary: String,
    #[serde(default)]
    pub proc_summary: String,
    #[serde(default)]
    pub gpu_rows: Vec<GpuRow>,
    #[serde(default)]
    pub mem_points: Vec<MemPoint>,
    #[serde(default)]
    pub events: Vec<LogEntry>,
    #[serde(default)]
    pub wer: Vec<LogEntry>,
    #[serde(default)]
    pub alerts: Vec<LogEntry>,
    #[serde(default)]
    pub alert_count: i32,
    #[serde(default)]
    pub startup_enabled: bool,
    #[serde(default)]
    pub full_history: bool,
    #[serde(default)]
    pub lan_url: String,
    #[serde(default)]
    pub ipv6_url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct ServerConfig {
    #[serde(default)]
    pub gpu_interval: f64,
    #[serde(default)]
    pub mem_interval: f64,
    #[serde(default)]
    pub proc_interval: f64,
    #[serde(default)]
    pub wer_interval: f64,
    #[serde(default)]
    pub game_names: Vec<String>,
    #[serde(default)]
    pub rotate_mb: i64,
    #[serde(default)]
    pub max_session_hours: i64,
    #[serde(default)]
    pub temp_warn: f64,
    #[serde(default)]
    pub hotspot_warn: f64,
    #[serde(default)]
    pub hotspot_delta_warn: f64,
    #[serde(default)]
    pub power_warn: f64,
    #[serde(default)]
    pub cpu_temp_warn: f64,
    #[serde(default)]
    pub cpu_usage_warn: f64,
    #[serde(default)]
    pub vram_warn_pct: f64,
    #[serde(default)]
    pub pcie_drop_warn: bool,
    #[serde(default)]
    pub gpu_index: i32,
    #[serde(default)]
    pub minimize_to_tray: bool,
    #[serde(default)]
    pub web_port: i32,
    #[serde(default)]
    pub auth_pin: String,
    #[serde(default)]
    pub read_only_remote: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct GpuDevice {
    #[serde(default)]
    pub index: i32,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct ConfigPayload {
    #[serde(default)]
    pub config: ServerConfig,
    #[serde(default)]
    pub gpus: Vec<GpuDevice>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct IncidentItem {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub belongs_to_session: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, uniffi::Record)]
pub struct DiscoveredHost {
    #[serde(default)]
    pub service: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: i32,
    #[serde(default)]
    pub lan_url: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PingResult {
    pub online: bool,
    pub latency_ms: i64,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum QrParseResult {
    Single { url: String },
    Dual { ipv4: String, ipv6: String },
    Invalid,
}

#[derive(Deserialize)]
struct QrPayloadJson {
    #[serde(default)]
    ipv4: String,
    #[serde(default)]
    ipv6: String,
}

#[derive(Deserialize)]
struct LiveSummaryResp {
    #[serde(default)]
    summary: String,
}

#[derive(Deserialize)]
struct LiveSaveResp {
    #[serde(default)]
    session: String,
}

#[derive(Deserialize)]
struct IncidentsResp {
    #[serde(default)]
    items: Vec<IncidentItem>,
}

#[derive(Deserialize)]
struct IncidentPreviewResp {
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: String,
}

#[derive(uniffi::Object)]
pub struct GpucapEngine {
    rt: Runtime,
    http: reqwest::Client,
    ping_http: reqwest::Client,
    pin: RwLock<String>,
}

#[uniffi::export]
impl GpucapEngine {
    #[uniffi::constructor]
    pub fn new() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("初始化 Tokio 运行时失败");
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(2500))
            .timeout(Duration::from_millis(5500))
            .build()
            .expect("初始化 Reqwest 客户端失败");
        let ping_http = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(1200))
            .timeout(Duration::from_millis(1800))
            .build()
            .expect("初始化测速客户端失败");
        Self {
            rt,
            http,
            ping_http,
            pin: RwLock::new(String::new()),
        }
    }

    pub fn set_pin(&self, pin: String) {
        if let Ok(mut guard) = self.pin.write() {
            *guard = pin.trim().to_string();
        }
    }

    pub fn normalize_url(&self, raw: String) -> Option<String> {
        normalize_url_inner(&raw)
    }

    pub fn parse_qr_payload(&self, raw: String) -> QrParseResult {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return QrParseResult::Invalid;
        }
        if trimmed.starts_with('{') {
            if let Ok(dto) = serde_json::from_str::<QrPayloadJson>(trimmed) {
                let v4 = normalize_url_inner(&dto.ipv4);
                let v6 = normalize_url_inner(&dto.ipv6);
                match (v4, v6) {
                    (Some(a), Some(b)) => return QrParseResult::Dual { ipv4: a, ipv6: b },
                    (Some(a), None) => return QrParseResult::Single { url: a },
                    (None, Some(b)) => return QrParseResult::Single { url: b },
                    (None, None) => return QrParseResult::Invalid,
                }
            }
        }
        if let Some(u) = normalize_url_inner(trimmed) {
            return QrParseResult::Single { url: u };
        }
        QrParseResult::Invalid
    }

    pub fn ping_host(&self, base_url: String) -> PingResult {
        let start = Instant::now();
        let ok = self.rt.block_on(async {
            let mut req = self.ping_http.get(format!("{}/api/state", base_url));
            if let Some(h) = self.pin_headers() {
                req = req.headers(h);
            }
            match req.send().await {
                Ok(resp) => resp.status().is_success() || resp.status().as_u16() == 401,
                Err(_) => false,
            }
        });
        PingResult {
            online: ok,
            latency_ms: start.elapsed().as_millis() as i64,
        }
    }

    pub fn discover_lan_hosts(&self, timeout_ms: u64) -> Vec<DiscoveredHost> {
        let mut found: HashMap<String, DiscoveredHost> = HashMap::new();
        let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
            return Vec::new();
        };
        let _ = sock.set_broadcast(true);
        let _ = sock.set_read_timeout(Some(Duration::from_millis(300)));
        let _ = sock.send_to(b"GPUCAP_DISCOVER", ("255.255.255.255", 8788));

        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(400));
        let mut buf = [0u8; 1024];
        while Instant::now() < deadline {
            if let Ok((n, _)) = sock.recv_from(&mut buf) {
                let raw = String::from_utf8_lossy(&buf[..n]);
                if let Ok(item) = serde_json::from_str::<DiscoveredHost>(raw.trim()) {
                    if !item.lan_url.trim().is_empty() {
                        found.insert(item.lan_url.clone(), item);
                    }
                }
            }
        }
        found.into_values().collect()
    }

    pub fn fetch_state(
        &self,
        base_url: String,
        session: String,
        full: bool,
    ) -> Result<StateSnapshot, GpucapCoreError> {
        let url = format!("{}/api/state", base_url);
        let full_s = if full { "1" } else { "0" };
        self.rt.block_on(async {
            let mut req = self
                .http
                .get(url)
                .query(&[("session", session.as_str()), ("full", full_s)]);
            if let Some(h) = self.pin_headers() {
                req = req.headers(h);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| GpucapCoreError::Network { msg: e.to_string() })?;
            parse_json_response(resp).await
        })
    }

    pub fn verify_pin(&self, base_url: String, pin: String) -> Result<(), GpucapCoreError> {
        self.rt.block_on(async {
            let resp = self
                .http
                .post(format!("{}/api/auth/verify", base_url))
                .json(&serde_json::json!({ "pin": pin.trim() }))
                .send()
                .await
                .map_err(|e| GpucapCoreError::Network { msg: e.to_string() })?;
            check_empty_response(resp).await
        })
    }

    pub fn set_collector(&self, base_url: String, start: bool) -> Result<(), GpucapCoreError> {
        let path = if start {
            "/api/collector/start"
        } else {
            "/api/collector/stop"
        };
        self.post_empty(&format!("{}{}", base_url, path), &serde_json::json!({}))
    }

    pub fn set_live_monitor(&self, base_url: String, start: bool) -> Result<(), GpucapCoreError> {
        let path = if start { "/api/live/start" } else { "/api/live/stop" };
        self.post_empty(&format!("{}{}", base_url, path), &serde_json::json!({}))
    }

    pub fn set_startup(&self, base_url: String, enabled: bool) -> Result<(), GpucapCoreError> {
        self.post_empty(
            &format!("{}/api/startup", base_url),
            &serde_json::json!({ "enabled": enabled }),
        )
    }

    pub fn fetch_live_summary(&self, base_url: String) -> Result<String, GpucapCoreError> {
        self.rt.block_on(async {
            let mut req = self.http.get(format!("{}/api/live/summary", base_url));
            if let Some(h) = self.pin_headers() {
                req = req.headers(h);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| GpucapCoreError::Network { msg: e.to_string() })?;
            let data: LiveSummaryResp = parse_json_response(resp).await?;
            Ok(data.summary)
        })
    }

    pub fn save_live_session(&self, base_url: String) -> Result<String, GpucapCoreError> {
        self.rt.block_on(async {
            let mut req = self
                .http
                .post(format!("{}/api/live/save", base_url))
                .json(&serde_json::json!({}));
            if let Some(h) = self.pin_headers() {
                req = req.headers(h);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| GpucapCoreError::Network { msg: e.to_string() })?;
            let data: LiveSaveResp = parse_json_response(resp).await?;
            Ok(data.session)
        })
    }

    pub fn fetch_config(&self, base_url: String) -> Result<ConfigPayload, GpucapCoreError> {
        self.rt.block_on(async {
            let mut req = self.http.get(format!("{}/api/config", base_url));
            if let Some(h) = self.pin_headers() {
                req = req.headers(h);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| GpucapCoreError::Network { msg: e.to_string() })?;
            parse_json_response(resp).await
        })
    }

    pub fn save_config(&self, base_url: String, cfg: ServerConfig) -> Result<(), GpucapCoreError> {
        self.post_empty(&format!("{}/api/config", base_url), &cfg)
    }

    pub fn delete_sessions(
        &self,
        base_url: String,
        names: Vec<String>,
    ) -> Result<(), GpucapCoreError> {
        self.post_empty(
            &format!("{}/api/sessions/delete", base_url),
            &serde_json::json!({ "names": names }),
        )
    }

    pub fn fetch_incidents(
        &self,
        base_url: String,
        session: String,
    ) -> Result<Vec<IncidentItem>, GpucapCoreError> {
        self.rt.block_on(async {
            let mut req = self
                .http
                .get(format!("{}/api/incidents", base_url))
                .query(&[("session", session.as_str())]);
            if let Some(h) = self.pin_headers() {
                req = req.headers(h);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| GpucapCoreError::Network { msg: e.to_string() })?;
            let data: IncidentsResp = parse_json_response(resp).await?;
            Ok(data.items)
        })
    }

    pub fn fetch_incident_preview(
        &self,
        base_url: String,
        name: String,
        file: String,
    ) -> Result<String, GpucapCoreError> {
        self.rt.block_on(async {
            let mut req = self
                .http
                .get(format!("{}/api/incidents/preview", base_url))
                .query(&[("name", name.as_str()), ("file", file.as_str())]);
            if let Some(h) = self.pin_headers() {
                req = req.headers(h);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| GpucapCoreError::Network { msg: e.to_string() })?;
            let data: IncidentPreviewResp = parse_json_response(resp).await?;
            Ok(data.text)
        })
    }
}

impl GpucapEngine {
    fn pin_headers(&self) -> Option<HeaderMap> {
        let guard = self.pin.read().ok()?;
        if guard.is_empty() {
            return None;
        }
        let mut map = HeaderMap::new();
        if let Ok(val) = HeaderValue::from_str(&guard) {
            map.insert("X-GPUCap-Pin", val);
        }
        Some(map)
    }

    fn post_empty<T: Serialize>(&self, url: &str, body: &T) -> Result<(), GpucapCoreError> {
        self.rt.block_on(async {
            let mut req = self.http.post(url).json(body);
            if let Some(h) = self.pin_headers() {
                req = req.headers(h);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| GpucapCoreError::Network { msg: e.to_string() })?;
            check_empty_response(resp).await
        })
    }
}

async fn parse_json_response<T: for<'de> Deserialize<'de>>(
    resp: reqwest::Response,
) -> Result<T, GpucapCoreError> {
    let status = resp.status();
    if status.as_u16() == 401 {
        return Err(GpucapCoreError::AuthRequired);
    }
    let text = resp
        .text()
        .await
        .map_err(|e| GpucapCoreError::Network { msg: e.to_string() })?;
    if !status.is_success() {
        let msg = serde_json::from_str::<ErrorBody>(&text)
            .map(|b| b.error)
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
        return Err(GpucapCoreError::Http {
            code: status.as_u16(),
            msg,
        });
    }
    serde_json::from_str::<T>(&text).map_err(|e| GpucapCoreError::Decode { msg: e.to_string() })
}

async fn check_empty_response(resp: reqwest::Response) -> Result<(), GpucapCoreError> {
    let status = resp.status();
    if status.as_u16() == 401 {
        return Err(GpucapCoreError::AuthRequired);
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<ErrorBody>(&text)
            .map(|b| b.error)
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
        return Err(GpucapCoreError::Http {
            code: status.as_u16(),
            msg,
        });
    }
    Ok(())
}

fn normalize_url_inner(raw: &str) -> Option<String> {
    let mut s = raw.trim().to_string();
    if s.is_empty() {
        return None;
    }
    if !s.starts_with("http://") && !s.starts_with("https://") {
        s = if s.contains(':') && !s.starts_with('[') && s.matches(':').count() > 1 {
            format!("http://[{}]:8787", s)
        } else {
            format!("http://{}", s)
        };
    }
    let trimmed = s.trim_end_matches('/');
    let after_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))?;
    if after_scheme.is_empty() {
        return None;
    }
    // 无端口时自动补全 :8787
    let host_part = after_scheme.split('/').next().unwrap_or(after_scheme);
    let has_port = if host_part.starts_with('[') {
        host_part.contains("]:")
    } else {
        host_part.contains(':')
    };
    if !has_port {
        Some(format!("{}:8787", trimmed))
    } else {
        Some(trimmed.to_string())
    }
}
