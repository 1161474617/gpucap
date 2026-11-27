package com.gpucap.nativeapp

import android.content.Context
import android.net.Uri
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.SocketTimeoutException
import java.net.URLEncoder
import java.util.concurrent.TimeUnit
import java.util.regex.Pattern

private val Context.nativeDataStore by preferencesDataStore(name = "gpucap_native_store")

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

@Serializable
private data class QrPayloadDto(
    val gpucap: Int = 1,
    val ipv4: String = "",
    val ipv6: String = "",
)

@Serializable
private data class StartupReqDto(val enabled: Boolean)

@Serializable
private data class DeleteReqDto(val names: List<String>)

class AuthRequiredException(message: String) : Exception(message)

/**
 * 负责本地 DataStore 存储与 GPUCap REST API 通信。
 */
class HostRepository(private val context: Context) {

    companion object {
        private val KEY_LAST_URL = stringPreferencesKey("last_url")
        private val KEY_HISTORY_JSON = stringPreferencesKey("history_urls_json")
        private val KEY_KEEP_SCREEN_ON = booleanPreferencesKey("keep_screen_on")
        private val KEY_AUTH_PIN = stringPreferencesKey("auth_pin")
        private val KEY_APP_LANG = stringPreferencesKey("app_lang")
        private const val DEFAULT_PORT = 8787
        private val JSON_MEDIA = "application/json; charset=utf-8".toMediaType()
        private val EMBEDDED_URL_PATTERN =
            Pattern.compile("https?://[^\\s\"'<>]+", Pattern.CASE_INSENSITIVE)
        private val HOST_PATTERN = Regex("[a-zA-Z0-9.\\-:\\[\\]]+")
    }

    val json = Json {
        ignoreUnknownKeys = true
        isLenient = true
        encodeDefaults = true
    }

    @Volatile
    private var cachedPin: String = ""

    private val httpClient = OkHttpClient.Builder()
        .connectTimeout(2500, TimeUnit.MILLISECONDS)
        .readTimeout(4500, TimeUnit.MILLISECONDS)
        .callTimeout(6000, TimeUnit.MILLISECONDS)
        .addInterceptor { chain ->
            val req = if (cachedPin.isNotBlank()) {
                chain.request().newBuilder().header("X-GPUCap-Pin", cachedPin).build()
            } else {
                chain.request()
            }
            chain.proceed(req)
        }
        .build()

    private val pingClient = OkHttpClient.Builder()
        .connectTimeout(1200, TimeUnit.MILLISECONDS)
        .readTimeout(1200, TimeUnit.MILLISECONDS)
        .callTimeout(1800, TimeUnit.MILLISECONDS)
        .build()

    val lastUrlFlow: Flow<String> = context.nativeDataStore.data.map { prefs ->
        prefs[KEY_LAST_URL] ?: ""
    }

    val historyFlow: Flow<List<String>> = context.nativeDataStore.data.map { prefs ->
        decodeHistory(prefs[KEY_HISTORY_JSON])
    }

    val appLangRawFlow: Flow<String> = context.nativeDataStore.data.map { prefs ->
        prefs[KEY_APP_LANG] ?: ""
    }

    suspend fun setAppLang(code: String) {
        context.nativeDataStore.edit { prefs ->
            prefs[KEY_APP_LANG] = code
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

    fun normalizeUrl(raw: String?): String? {
        if (raw.isNullOrBlank()) return null
        var s = raw.trim()
        val m = EMBEDDED_URL_PATTERN.matcher(s)
        if (m.find()) {
            s = m.group()
        }
        if (!s.startsWith("http://", ignoreCase = true) &&
            !s.startsWith("https://", ignoreCase = true)
        ) {
            s = "http://$s"
        }
        while (s.endsWith("/")) {
            s = s.dropLast(1)
        }
        return try {
            val uri = Uri.parse(s)
            val host = uri.host
            if (host.isNullOrEmpty() || !HOST_PATTERN.matches(host)) {
                null
            } else if (uri.port == -1 && uri.path.isNullOrEmpty()) {
                "$s:$DEFAULT_PORT"
            } else {
                s
            }
        } catch (_: Exception) {
            null
        }
    }

    fun parseQrPayload(contents: String): QrParsedResult {
        val trimmed = contents.trim()
        if (trimmed.startsWith("{")) {
            runCatching { json.decodeFromString<QrPayloadDto>(trimmed) }.getOrNull()?.let { dto ->
                val v4 = normalizeUrl(dto.ipv4)
                val v6 = normalizeUrl(dto.ipv6)
                if (v4 != null && v6 != null) return QrParsedResult.Dual(v4, v6)
                if (v4 != null) return QrParsedResult.Single(v4)
                if (v6 != null) return QrParsedResult.Single(v6)
            }
        }
        val single = normalizeUrl(trimmed) ?: return QrParsedResult.Invalid
        return QrParsedResult.Single(single)
    }

    suspend fun pingUrl(baseUrl: String): PingStatus = withContext(Dispatchers.IO) {
        val t0 = System.nanoTime()
        try {
            val req = Request.Builder().url("$baseUrl/api/state").get().build()
            pingClient.newCall(req).execute().use { resp ->
                if (resp.isSuccessful) {
                    val ms = ((System.nanoTime() - t0) / 1_000_000L).coerceAtLeast(1L)
                    PingStatus.Online(ms)
                } else {
                    PingStatus.Offline
                }
            }
        } catch (_: Exception) {
            PingStatus.Offline
        }
    }

    // MARK: - 原生监控 REST API

    suspend fun fetchState(baseUrl: String, session: String, full: Boolean): Result<StateResponse> =
        withContext(Dispatchers.IO) {
            runCatching {
                val encSess = URLEncoder.encode(session, "UTF-8")
                val url = "$baseUrl/api/state?session=$encSess&full=${if (full) "1" else "0"}"
                val req = Request.Builder().url(url).get().build()
                httpClient.newCall(req).execute().use { resp ->
                    if (resp.code == 401) throw AuthRequiredException("需要主机访问口令")
                    if (!resp.isSuccessful) error("HTTP ${resp.code}")
                    val body = resp.body?.string() ?: error("空响应")
                    json.decodeFromString<StateResponse>(body)
                }
            }
        }

    suspend fun setCollector(baseUrl: String, start: Boolean): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatching {
                val path = if (start) "/api/collector/start" else "/api/collector/stop"
                val req = Request.Builder()
                    .url("$baseUrl$path")
                    .post("{}".toRequestBody(JSON_MEDIA))
                    .build()
                httpClient.newCall(req).execute().use { resp ->
                    if (!resp.isSuccessful) error("操作失败 (HTTP ${resp.code})")
                }
            }
        }

    val keepScreenOnFlow: Flow<Boolean> = context.nativeDataStore.data.map { prefs ->
        prefs[KEY_KEEP_SCREEN_ON] ?: false
    }

    val authPinFlow: Flow<String> = context.nativeDataStore.data.map { prefs ->
        val p = prefs[KEY_AUTH_PIN] ?: ""
        cachedPin = p
        p
    }

    suspend fun saveAuthPin(pin: String) {
        cachedPin = pin.trim()
        context.nativeDataStore.edit { prefs ->
            prefs[KEY_AUTH_PIN] = cachedPin
        }
    }

    suspend fun verifyPin(baseUrl: String, pin: String): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatching {
                val payload = json.encodeToString(AuthVerifyReqDto(pin.trim()))
                val req = Request.Builder()
                    .url("$baseUrl/api/auth/verify")
                    .post(payload.toRequestBody(JSON_MEDIA))
                    .build()
                httpClient.newCall(req).execute().use { resp ->
                    if (!resp.isSuccessful) error("口令错误 (HTTP ${resp.code})")
                }
            }
        }

    suspend fun setKeepScreenOn(enabled: Boolean) {
        context.nativeDataStore.edit { prefs ->
            prefs[KEY_KEEP_SCREEN_ON] = enabled
        }
    }

    suspend fun discoverLanHosts(timeoutMs: Int = 1200): List<DiscoveredHost> =
        withContext(Dispatchers.IO) {
            val found = LinkedHashMap<String, DiscoveredHost>()
            runCatching {
                DatagramSocket().use { sock ->
                    sock.broadcast = true
                    sock.soTimeout = 350
                    val probe = "GPUCAP_DISCOVER".toByteArray(Charsets.UTF_8)
                    val packet = DatagramPacket(
                        probe,
                        probe.size,
                        InetAddress.getByName("255.255.255.255"),
                        8788,
                    )
                    sock.send(packet)
                    val deadline = System.currentTimeMillis() + timeoutMs
                    val buf = ByteArray(1024)
                    while (System.currentTimeMillis() < deadline) {
                        try {
                            val resp = DatagramPacket(buf, buf.size)
                            sock.receive(resp)
                            val raw = String(resp.data, 0, resp.length, Charsets.UTF_8).trim()
                            val item = json.decodeFromString<DiscoveredHost>(raw)
                            if (item.lanUrl.isNotBlank()) {
                                found[item.lanUrl] = item
                            }
                        } catch (_: SocketTimeoutException) {
                            // 继续等待直到总超时
                        }
                    }
                }
            }
            found.values.toList()
        }

    suspend fun fetchLiveSummary(baseUrl: String): Result<String> =
        withContext(Dispatchers.IO) {
            runCatching {
                val req = Request.Builder().url("$baseUrl/api/live/summary").get().build()
                httpClient.newCall(req).execute().use { resp ->
                    if (!resp.isSuccessful) error("HTTP ${resp.code}")
                    val body = resp.body?.string() ?: error("空响应")
                    json.decodeFromString<LiveSummaryResponse>(body).summary
                }
            }
        }

    suspend fun saveLiveSession(baseUrl: String): Result<String> =
        withContext(Dispatchers.IO) {
            runCatching {
                val req = Request.Builder()
                    .url("$baseUrl/api/live/save")
                    .post("{}".toRequestBody(JSON_MEDIA))
                    .build()
                httpClient.newCall(req).execute().use { resp ->
                    if (!resp.isSuccessful) error("保存会话失败 (HTTP ${resp.code})")
                    val body = resp.body?.string() ?: error("空响应")
                    json.decodeFromString<LiveSaveResponse>(body).session
                }
            }
        }

    suspend fun setLiveMonitor(baseUrl: String, start: Boolean): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatching {
                val path = if (start) "/api/live/start" else "/api/live/stop"
                val req = Request.Builder()
                    .url("$baseUrl$path")
                    .post("{}".toRequestBody(JSON_MEDIA))
                    .build()
                httpClient.newCall(req).execute().use { resp ->
                    if (!resp.isSuccessful) error("实时监测操作失败 (HTTP ${resp.code})")
                }
            }
        }

    suspend fun setStartup(baseUrl: String, enabled: Boolean): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatching {
                val body = json.encodeToString(StartupReqDto(enabled)).toRequestBody(JSON_MEDIA)
                val req = Request.Builder().url("$baseUrl/api/startup").post(body).build()
                httpClient.newCall(req).execute().use { resp ->
                    if (!resp.isSuccessful) error("设置开机自启失败 (HTTP ${resp.code})")
                }
            }
        }

    suspend fun fetchConfig(baseUrl: String): Result<ConfigResponse> =
        withContext(Dispatchers.IO) {
            runCatching {
                val req = Request.Builder().url("$baseUrl/api/config").get().build()
                httpClient.newCall(req).execute().use { resp ->
                    if (!resp.isSuccessful) error("读取配置失败 (HTTP ${resp.code})")
                    val body = resp.body?.string() ?: error("空响应")
                    json.decodeFromString<ConfigResponse>(body)
                }
            }
        }

    suspend fun saveConfig(baseUrl: String, cfg: ServerConfigDto): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatching {
                val body = json.encodeToString(cfg).toRequestBody(JSON_MEDIA)
                val req = Request.Builder().url("$baseUrl/api/config").post(body).build()
                httpClient.newCall(req).execute().use { resp ->
                    if (!resp.isSuccessful) error("保存配置失败 (HTTP ${resp.code})")
                }
            }
        }

    suspend fun deleteSession(baseUrl: String, name: String): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatching {
                val body = json.encodeToString(DeleteReqDto(listOf(name))).toRequestBody(JSON_MEDIA)
                val req = Request.Builder().url("$baseUrl/api/sessions/delete").post(body).build()
                httpClient.newCall(req).execute().use { resp ->
                    if (!resp.isSuccessful) error("删除失败 (HTTP ${resp.code})")
                }
            }
        }

    suspend fun fetchIncidents(baseUrl: String, session: String): Result<List<IncidentItemDto>> =
        withContext(Dispatchers.IO) {
            runCatching {
                val enc = URLEncoder.encode(session, "UTF-8")
                val req = Request.Builder().url("$baseUrl/api/incidents?session=$enc").get().build()
                httpClient.newCall(req).execute().use { resp ->
                    if (!resp.isSuccessful) error("HTTP ${resp.code}")
                    val body = resp.body?.string() ?: "{}"
                    json.decodeFromString<IncidentListResponse>(body).items
                }
            }
        }

    suspend fun fetchIncidentPreview(baseUrl: String, name: String, file: String): Result<String> =
        withContext(Dispatchers.IO) {
            runCatching {
                val en = URLEncoder.encode(name, "UTF-8")
                val ef = URLEncoder.encode(file, "UTF-8")
                val req = Request.Builder()
                    .url("$baseUrl/api/incidents/preview?name=$en&file=$ef")
                    .get()
                    .build()
                httpClient.newCall(req).execute().use { resp ->
                    if (!resp.isSuccessful) error("HTTP ${resp.code}")
                    val body = resp.body?.string() ?: "{}"
                    json.decodeFromString<IncidentPreviewResponse>(body).text
                }
            }
        }
}
