package com.gpucap.nativeapp

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class HeartbeatDto(
    val text: String = "● 未采集",
    val level: String = "dim",
)

@Serializable
data class SessionMetaDto(
    val name: String = "",
    @SerialName("size_label") val sizeLabel: String = "",
    val active: Boolean = false,
)

@Serializable
data class GpuRowDto(
    val ts: String = "",
    val temp: String = "",
    val hotspot: String = "",
    val power: String = "",
    val core: String = "",
    val memclk: String = "",
    val pcie: String = "",
    val util: String = "",
    val vram: String = "",
    val fan: String = "",
    val thr: String = "",
    @SerialName("thr_label") val thrLabel: String = "",
    val ok: String = "",
    val sev: String = "normal",
) {
    val hotspotDeltaVal: Double?
        get() {
            val hs = hotspot.toDoubleOrNull() ?: return null
            val t = temp.toDoubleOrNull() ?: return null
            return hs - t
        }

    val hotspotDeltaText: String?
        get() = hotspotDeltaVal?.let { String.format(java.util.Locale.US, "%.1f", it) }
}

@Serializable
data class MemPointDto(
    val ts: String = "",
    @SerialName("commit_pct") val commitPct: Double = 0.0,
    @SerialName("nonpaged_mb") val nonpagedMb: Double = 0.0,
)

@Serializable
data class LogEntryDto(
    val time: String = "",
    val level: String = "",
    val source: String = "",
    val message: String = "",
    val raw: String = "",
    val sev: String = "normal",
)

@Serializable
data class CpuCoreInfoDto(
    val id: Int = 0,
    val kind: String = "Std",
    val usage: Double = 0.0,
    @SerialName("freq_mhz") val freqMhz: Long = 0L,
)

@Serializable
data class TopProcessDto(
    val pid: Long = 0L,
    val name: String = "",
    @SerialName("cpu_pct") val cpuPct: Double = 0.0,
    @SerialName("mem_mb") val memMb: Long = 0L,
)

@Serializable
data class LiveCpuPointDto(
    val ts: String = "",
    @SerialName("total_usage") val totalUsage: Double = 0.0,
    @SerialName("avg_freq_mhz") val avgFreqMhz: Long = 0L,
    @SerialName("temp_c") val tempC: Double? = null,
    val alert: Boolean = false,
)

@Serializable
data class DiscoveredHost(
    val service: String = "gpucap",
    val host: String = "",
    val port: Int = 8787,
    @SerialName("lan_url") val lanUrl: String = "",
)

@Serializable
data class LiveSummaryResponse(
    val ok: Boolean = false,
    val summary: String = "",
)

@Serializable
data class LiveSaveResponse(
    val ok: Boolean = false,
    val session: String = "",
)

@Serializable
data class CpuSnapshotDto(
    val brand: String = "",
    @SerialName("temp_c") val tempC: Double? = null,
    @SerialName("total_usage") val totalUsage: Double = 0.0,
    @SerialName("avg_freq_mhz") val avgFreqMhz: Long = 0L,
    @SerialName("max_freq_mhz") val maxFreqMhz: Long = 0L,
    @SerialName("physical_cores") val physicalCores: Int = 0,
    @SerialName("logical_cores") val logicalCores: Int = 0,
    @SerialName("has_hybrid") val hasHybrid: Boolean = false,
    @SerialName("p_count") val pCount: Int = 0,
    @SerialName("p_usage") val pUsage: Double = 0.0,
    @SerialName("p_freq_mhz") val pFreqMhz: Long = 0L,
    @SerialName("e_count") val eCount: Int = 0,
    @SerialName("e_usage") val eUsage: Double = 0.0,
    @SerialName("e_freq_mhz") val eFreqMhz: Long = 0L,
    val cores: List<CpuCoreInfoDto> = emptyList(),
    @SerialName("top_cpu_procs") val topCpuProcs: List<TopProcessDto> = emptyList(),
    @SerialName("top_mem_procs") val topMemProcs: List<TopProcessDto> = emptyList(),
)

@Serializable
data class MetricStatsDto(
    val min: Double = 0.0,
    val avg: Double = 0.0,
    val max: Double = 0.0,
    val count: Int = 0,
)

@Serializable
data class LiveStatsSummaryDto(
    @SerialName("gpu_temp") val gpuTemp: MetricStatsDto? = null,
    @SerialName("gpu_hotspot") val gpuHotspot: MetricStatsDto? = null,
    @SerialName("gpu_power") val gpuPower: MetricStatsDto? = null,
    @SerialName("gpu_core") val gpuCore: MetricStatsDto? = null,
    @SerialName("cpu_usage") val cpuUsage: MetricStatsDto? = null,
    @SerialName("cpu_temp") val cpuTemp: MetricStatsDto? = null,
)

@Serializable
data class AuthVerifyReqDto(val pin: String)

@Serializable
data class StateResponse(
    @SerialName("collector_running") val collectorRunning: Boolean = false,
    @SerialName("live_running") val liveRunning: Boolean = false,
    @SerialName("live_has_data") val liveHasData: Boolean = false,
    val cpu: CpuSnapshotDto? = null,
    @SerialName("cpu_points") val cpuPoints: List<LiveCpuPointDto> = emptyList(),
    val stats: LiveStatsSummaryDto = LiveStatsSummaryDto(),
    @SerialName("read_only_remote") val readOnlyRemote: Boolean = false,
    val heartbeat: HeartbeatDto = HeartbeatDto(),
    val session: String = "",
    val sessions: List<SessionMetaDto> = emptyList(),
    @SerialName("gpu_summary") val gpuSummary: String = "",
    @SerialName("mem_summary") val memSummary: String = "",
    @SerialName("proc_summary") val procSummary: String = "",
    @SerialName("gpu_rows") val gpuRows: List<GpuRowDto> = emptyList(),
    @SerialName("mem_points") val memPoints: List<MemPointDto> = emptyList(),
    val events: List<LogEntryDto> = emptyList(),
    val wer: List<LogEntryDto> = emptyList(),
    val alerts: List<LogEntryDto> = emptyList(),
    @SerialName("alert_count") val alertCount: Int = 0,
    @SerialName("startup_enabled") val startupEnabled: Boolean = false,
    @SerialName("full_history") val fullHistory: Boolean = false,
    @SerialName("lan_url") val lanUrl: String = "",
    @SerialName("ipv6_url") val ipv6Url: String = "",
)

@Serializable
data class ServerConfigDto(
    @SerialName("gpu_interval") val gpuInterval: Double = 1.0,
    @SerialName("mem_interval") val memInterval: Double = 5.0,
    @SerialName("proc_interval") val procInterval: Double = 5.0,
    @SerialName("wer_interval") val werInterval: Double = 5.0,
    @SerialName("game_names") val gameNames: List<String> = emptyList(),
    @SerialName("rotate_mb") val rotateMb: Long = 64,
    @SerialName("max_session_hours") val maxSessionHours: Long = 24,
    @SerialName("temp_warn") val tempWarn: Double = 78.0,
    @SerialName("hotspot_warn") val hotspotWarn: Double = 92.0,
    @SerialName("hotspot_delta_warn") val hotspotDeltaWarn: Double = 20.0,
    @SerialName("power_warn") val powerWarn: Double = 360.0,
    @SerialName("cpu_temp_warn") val cpuTempWarn: Double = 90.0,
    @SerialName("cpu_usage_warn") val cpuUsageWarn: Double = 95.0,
    @SerialName("gpu_index") val gpuIndex: Int = 0,
    @SerialName("minimize_to_tray") val minimizeToTray: Boolean = true,
    @SerialName("web_port") val webPort: Int = 8787,
    @SerialName("auth_pin") val authPin: String = "",
    @SerialName("read_only_remote") val readOnlyRemote: Boolean = false,
)

@Serializable
data class GpuDeviceDto(
    val index: Int = 0,
    val name: String = "",
)

@Serializable
data class ConfigResponse(
    val config: ServerConfigDto = ServerConfigDto(),
    val gpus: List<GpuDeviceDto> = emptyList(),
)

@Serializable
data class IncidentItemDto(
    val name: String = "",
    val reason: String = "",
    @SerialName("belongs_to_session") val belongsToSession: Boolean = false,
)

@Serializable
data class IncidentListResponse(
    val items: List<IncidentItemDto> = emptyList(),
)

@Serializable
data class IncidentPreviewResponse(
    val text: String = "",
)
