package com.gpucap.nativeapp

import android.content.Context
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import uniffi.gpucap_core.GpucapCoreException
import uniffi.gpucap_core.GpucapEngine
import uniffi.gpucap_core.QrParseResult as RustQrResult
import uniffi.gpucap_core.ServerConfig as RustServerConfig
import uniffi.gpucap_core.StateSnapshot as RustState

private val Context.nativeDataStore by preferencesDataStore(name = "gpucap_rust_store")

sealed interface PingStatus {
    data object Idle : PingStatus
    data object Checking : PingStatus
    data class Online(val latencyMs: Long) : PingStatus
    data object Offline : PingStatus
}

sealed interface QrParsedResult {
    data class Single(val url: String) : QrParsedResult
    data class Dual(val ipv4: String, val ipv6: String) : QrParsedResult
    data object Invalid : QrParsedResult
}

class AuthRequiredException(message: String) : Exception(message)

/**
 * 基于 Rust 核心引擎（UniFFI + Tokio + Reqwest）驱动的仓库层，
 * 负责本地 DataStore 偏好持久化与 Rust 异步通信桥接。
 */
class HostRepository(private val context: Context) {

    companion object {
        private val KEY_LAST_URL = stringPreferencesKey("last_url")
        private val KEY_HISTORY_JSON = stringPreferencesKey("history_urls_json")
        private val KEY_KEEP_SCREEN_ON = booleanPreferencesKey("keep_screen_on")
        private val KEY_AUTH_PIN = stringPreferencesKey("auth_pin")
    }

    private val engine = GpucapEngine()
    private val json = Json {
        ignoreUnknownKeys = true
        isLenient = true
        encodeDefaults = true
    }

    val lastUrlFlow: Flow<String> = context.nativeDataStore.data.map { prefs ->
        prefs[KEY_LAST_URL] ?: ""
    }

    val historyFlow: Flow<List<String>> = context.nativeDataStore.data.map { prefs ->
        decodeHistory(prefs[KEY_HISTORY_JSON])
    }

    val keepScreenOnFlow: Flow<Boolean> = context.nativeDataStore.data.map { prefs ->
        prefs[KEY_KEEP_SCREEN_ON] ?: false
    }

    val authPinFlow: Flow<String> = context.nativeDataStore.data.map { prefs ->
        val p = prefs[KEY_AUTH_PIN] ?: ""
        engine.setPin(p)
        p
    }

    suspend fun setKeepScreenOn(enabled: Boolean) {
        context.nativeDataStore.edit { prefs ->
            prefs[KEY_KEEP_SCREEN_ON] = enabled
        }
    }

    suspend fun saveAuthPin(pin: String) {
        val clean = pin.trim()
        engine.setPin(clean)
        context.nativeDataStore.edit { prefs ->
            prefs[KEY_AUTH_PIN] = clean
        }
    }

    suspend fun saveAndRecord(url: String) {
        context.nativeDataStore.edit { prefs ->
            prefs[KEY_LAST_URL] = url
            val list = decodeHistory(prefs[KEY_HISTORY_JSON]).toMutableList()
            list.remove(url)
            list.add(0, url)
            prefs[KEY_HISTORY_JSON] = json.encodeToString(list.take(8))
        }
    }

    suspend fun addHistoryOnly(url: String) {
        context.nativeDataStore.edit { prefs ->
            val list = decodeHistory(prefs[KEY_HISTORY_JSON]).toMutableList()
            list.remove(url)
            list.add(0, url)
            prefs[KEY_HISTORY_JSON] = json.encodeToString(list.take(8))
        }
    }

    suspend fun removeHistory(url: String) {
        context.nativeDataStore.edit { prefs ->
            val list = decodeHistory(prefs[KEY_HISTORY_JSON]).toMutableList()
            list.remove(url)
            prefs[KEY_HISTORY_JSON] = json.encodeToString(list)
        }
    }

    private fun decodeHistory(raw: String?): List<String> {
        if (raw.isNullOrBlank()) return emptyList()
        return runCatching { json.decodeFromString<List<String>>(raw) }.getOrDefault(emptyList())
    }

    fun normalizeUrl(raw: String?): String? =
        raw?.let { engine.normalizeUrl(it) }

    fun parseQrPayload(raw: String?): QrParsedResult {
        if (raw.isNullOrBlank()) return QrParsedResult.Invalid
        return when (val r = engine.parseQrPayload(raw)) {
            is RustQrResult.Single -> QrParsedResult.Single(r.url)
            is RustQrResult.Dual -> QrParsedResult.Dual(r.ipv4, r.ipv6)
            RustQrResult.Invalid -> QrParsedResult.Invalid
        }
    }

    suspend fun pingUrl(baseUrl: String): PingStatus = withContext(Dispatchers.IO) {
        val res = engine.pingHost(baseUrl)
        if (res.online) PingStatus.Online(res.latencyMs) else PingStatus.Offline
    }

    suspend fun discoverLanHosts(timeoutMs: Int = 1200): List<DiscoveredHost> =
        withContext(Dispatchers.IO) {
            runCatching {
                engine.discoverLanHosts(timeoutMs.toULong()).map {
                    DiscoveredHost(
                        service = it.service,
                        host = it.host,
                        port = it.port,
                        lanUrl = it.lanUrl,
                    )
                }
            }.getOrDefault(emptyList())
        }

    suspend fun fetchState(baseUrl: String, session: String, full: Boolean): Result<StateResponse> =
        withContext(Dispatchers.IO) {
            runCatchingRust {
                mapState(engine.fetchState(baseUrl, session, full))
            }
        }

    suspend fun verifyPin(baseUrl: String, pin: String): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatchingRust {
                engine.verifyPin(baseUrl, pin)
            }
        }

    suspend fun setCollector(baseUrl: String, start: Boolean): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatchingRust {
                engine.setCollector(baseUrl, start)
            }
        }

    suspend fun setLiveMonitor(baseUrl: String, start: Boolean): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatchingRust {
                engine.setLiveMonitor(baseUrl, start)
            }
        }

    suspend fun fetchLiveSummary(baseUrl: String): Result<String> =
        withContext(Dispatchers.IO) {
            runCatchingRust {
                engine.fetchLiveSummary(baseUrl)
            }
        }

    suspend fun saveLiveSession(baseUrl: String): Result<String> =
        withContext(Dispatchers.IO) {
            runCatchingRust {
                engine.saveLiveSession(baseUrl)
            }
        }

    suspend fun setStartup(baseUrl: String, enabled: Boolean): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatchingRust {
                engine.setStartup(baseUrl, enabled)
            }
        }

    suspend fun fetchConfig(baseUrl: String): Result<ConfigResponse> =
        withContext(Dispatchers.IO) {
            runCatchingRust {
                val res = engine.fetchConfig(baseUrl)
                val c = res.config
                ConfigResponse(
                    config = ServerConfigDto(
                        gpuInterval = c.gpuInterval,
                        memInterval = c.memInterval,
                        procInterval = c.procInterval,
                        werInterval = c.werInterval,
                        gameNames = c.gameNames,
                        rotateMb = c.rotateMb,
                        maxSessionHours = c.maxSessionHours,
                        tempWarn = c.tempWarn,
                        hotspotWarn = c.hotspotWarn,
                        hotspotDeltaWarn = c.hotspotDeltaWarn,
                        powerWarn = c.powerWarn,
                        cpuTempWarn = c.cpuTempWarn,
                        cpuUsageWarn = c.cpuUsageWarn,
                        vramWarnPct = c.vramWarnPct,
                        pcieDropWarn = c.pcieDropWarn,
                        gpuIndex = c.gpuIndex,
                        minimizeToTray = c.minimizeToTray,
                        webPort = c.webPort,
                        authPin = c.authPin,
                        readOnlyRemote = c.readOnlyRemote,
                    ),
                    gpus = res.gpus.map { GpuDeviceDto(index = it.index, name = it.name) },
                )
            }
        }

    suspend fun saveConfig(baseUrl: String, cfg: ServerConfigDto): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatchingRust {
                engine.saveConfig(
                    baseUrl,
                    RustServerConfig(
                        gpuInterval = cfg.gpuInterval,
                        memInterval = cfg.memInterval,
                        procInterval = cfg.procInterval,
                        werInterval = cfg.werInterval,
                        gameNames = cfg.gameNames,
                        rotateMb = cfg.rotateMb,
                        maxSessionHours = cfg.maxSessionHours,
                        tempWarn = cfg.tempWarn,
                        hotspotWarn = cfg.hotspotWarn,
                        hotspotDeltaWarn = cfg.hotspotDeltaWarn,
                        powerWarn = cfg.powerWarn,
                        cpuTempWarn = cfg.cpuTempWarn,
                        cpuUsageWarn = cfg.cpuUsageWarn,
                        vramWarnPct = cfg.vramWarnPct,
                        pcieDropWarn = cfg.pcieDropWarn,
                        gpuIndex = cfg.gpuIndex,
                        minimizeToTray = cfg.minimizeToTray,
                        webPort = cfg.webPort,
                        authPin = cfg.authPin,
                        readOnlyRemote = cfg.readOnlyRemote,
                    ),
                )
            }
        }

    suspend fun deleteSession(baseUrl: String, name: String): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatchingRust {
                engine.deleteSessions(baseUrl, listOf(name))
            }
        }

    suspend fun fetchIncidents(baseUrl: String, session: String): Result<List<IncidentItemDto>> =
        withContext(Dispatchers.IO) {
            runCatchingRust {
                engine.fetchIncidents(baseUrl, session).map {
                    IncidentItemDto(name = it.name, reason = it.reason, belongsToSession = it.belongsToSession)
                }
            }
        }

    suspend fun fetchIncidentPreview(baseUrl: String, name: String, file: String): Result<String> =
        withContext(Dispatchers.IO) {
            runCatchingRust {
                engine.fetchIncidentPreview(baseUrl, name, file)
            }
        }

    private inline fun <T> runCatchingRust(block: () -> T): Result<T> =
        try {
            Result.success(block())
        } catch (_: GpucapCoreException.AuthRequired) {
            Result.failure(AuthRequiredException("需要主机访问口令"))
        } catch (e: GpucapCoreException) {
            Result.failure(Exception(e.message ?: "Rust 核心异常"))
        } catch (t: Throwable) {
            Result.failure(t)
        }

    private fun mapState(s: RustState): StateResponse = StateResponse(
        collectorRunning = s.collectorRunning,
        liveRunning = s.liveRunning,
        liveHasData = s.liveHasData,
        cpu = s.cpu?.let { c ->
            CpuSnapshotDto(
                brand = c.brand,
                tempC = c.tempC,
                totalUsage = c.totalUsage,
                avgFreqMhz = c.avgFreqMhz,
                maxFreqMhz = c.maxFreqMhz,
                physicalCores = c.physicalCores,
                logicalCores = c.logicalCores,
                hasHybrid = c.hasHybrid,
                pCount = c.pCount,
                pUsage = c.pUsage,
                pFreqMhz = c.pFreqMhz,
                eCount = c.eCount,
                eUsage = c.eUsage,
                eFreqMhz = c.eFreqMhz,
                cores = c.cores.map {
                    CpuCoreInfoDto(id = it.id, kind = it.kind, usage = it.usage, freqMhz = it.freqMhz)
                },
                topCpuProcs = c.topCpuProcs.map {
                    TopProcessDto(pid = it.pid, name = it.name, cpuPct = it.cpuPct, memMb = it.memMb)
                },
                topMemProcs = c.topMemProcs.map {
                    TopProcessDto(pid = it.pid, name = it.name, cpuPct = it.cpuPct, memMb = it.memMb)
                },
            )
        },
        cpuPoints = s.cpuPoints.map {
            LiveCpuPointDto(ts = it.ts, totalUsage = it.totalUsage, avgFreqMhz = it.avgFreqMhz, tempC = it.tempC, alert = it.alert)
        },
        stats = LiveStatsSummaryDto(
            gpuTemp = s.stats.gpuTemp?.let { MetricStatsDto(it.min, it.avg, it.max, it.count) },
            gpuHotspot = s.stats.gpuHotspot?.let { MetricStatsDto(it.min, it.avg, it.max, it.count) },
            gpuPower = s.stats.gpuPower?.let { MetricStatsDto(it.min, it.avg, it.max, it.count) },
            gpuCore = s.stats.gpuCore?.let { MetricStatsDto(it.min, it.avg, it.max, it.count) },
            cpuUsage = s.stats.cpuUsage?.let { MetricStatsDto(it.min, it.avg, it.max, it.count) },
            cpuTemp = s.stats.cpuTemp?.let { MetricStatsDto(it.min, it.avg, it.max, it.count) },
        ),
        env = s.env?.let {
            EnvFingerprintDto(
                gpuName = it.gpuName,
                driverVersion = it.driverVersion,
                vramTotalMb = it.vramTotalMb,
                pcieMax = it.pcieMax,
                vbios = it.vbios,
                osVersion = it.osVersion,
                powerPlan = it.powerPlan,
                driverChangedFrom = it.driverChangedFrom,
            )
        },
        readOnlyRemote = s.readOnlyRemote,
        heartbeat = HeartbeatDto(text = s.heartbeat.text, level = s.heartbeat.level),
        session = s.session,
        sessions = s.sessions.map {
            SessionMetaDto(name = it.name, sizeLabel = it.sizeLabel, active = it.active)
        },
        gpuSummary = s.gpuSummary,
        memSummary = s.memSummary,
        procSummary = s.procSummary,
        gpuRows = s.gpuRows.map {
            GpuRowDto(
                ts = it.ts,
                temp = it.temp,
                hotspot = it.hotspot,
                power = it.power,
                core = it.core,
                memclk = it.memclk,
                pcie = it.pcie,
                util = it.util,
                vram = it.vram,
                vramTotal = it.vramTotal,
                vramPct = it.vramPct,
                pcieDrop = it.pcieDrop,
                fan = it.fan,
                thr = it.thr,
                thrLabel = it.thrLabel,
                ok = it.ok,
                sev = it.sev,
            )
        },
        memPoints = s.memPoints.map {
            MemPointDto(ts = it.ts, commitPct = it.commitPct, nonpagedMb = it.nonpagedMb)
        },
        events = s.events.map {
            LogEntryDto(time = it.time, level = it.level, source = it.source, message = it.message, raw = it.raw, sev = it.sev)
        },
        wer = s.wer.map {
            LogEntryDto(time = it.time, level = it.level, source = it.source, message = it.message, raw = it.raw, sev = it.sev)
        },
        alerts = s.alerts.map {
            LogEntryDto(time = it.time, level = it.level, source = it.source, message = it.message, raw = it.raw, sev = it.sev)
        },
        alertCount = s.alertCount,
        startupEnabled = s.startupEnabled,
        fullHistory = s.fullHistory,
        lanUrl = s.lanUrl,
        ipv6Url = s.ipv6Url,
    )
}
