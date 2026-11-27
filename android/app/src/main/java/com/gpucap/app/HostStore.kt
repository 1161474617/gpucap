package com.gpucap.app

import android.content.Context
import android.net.Uri
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import okhttp3.Request
import java.util.concurrent.TimeUnit
import java.util.regex.Pattern

private val Context.gpucapDataStore by preferencesDataStore(name = "gpucap_store")

/** 单条历史地址的连通性状态 */
sealed interface PingStatus {
    data object Idle : PingStatus
    data object Checking : PingStatus
    data class Online(val latencyMs: Long) : PingStatus
    data object Offline : PingStatus
}

/** 二维码强类型解析结果 */
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

/**
 * 基于 Jetpack DataStore + kotlinx.serialization + OkHttp 的主机配置与测速仓库。
 */
class HostRepository(private val context: Context) {

    companion object {
        private val KEY_LAST_URL = stringPreferencesKey("last_url")
        private val KEY_HISTORY_JSON = stringPreferencesKey("history_urls_json")
        private val KEY_APP_LANG = stringPreferencesKey("app_lang")
        private const val LEGACY_PREFS = "gpucap_prefs"
        private const val DEFAULT_PORT = 8787
        private val EMBEDDED_URL_PATTERN =
            Pattern.compile("https?://[^\\s\"'<>]+", Pattern.CASE_INSENSITIVE)
        private val HOST_PATTERN = Regex("[a-zA-Z0-9.\\-:\\[\\]]+")
    }

    private val json = Json {
        ignoreUnknownKeys = true
        isLenient = true
    }

    private val httpClient = OkHttpClient.Builder()
        .connectTimeout(1200, TimeUnit.MILLISECONDS)
        .readTimeout(1200, TimeUnit.MILLISECONDS)
        .callTimeout(1800, TimeUnit.MILLISECONDS)
        .build()

    val lastUrlFlow: Flow<String> = context.gpucapDataStore.data.map { prefs ->
        prefs[KEY_LAST_URL] ?: ""
    }

    val historyFlow: Flow<List<String>> = context.gpucapDataStore.data.map { prefs ->
        decodeHistory(prefs[KEY_HISTORY_JSON])
    }

    val appLangRawFlow: Flow<String> = context.gpucapDataStore.data.map { prefs ->
        prefs[KEY_APP_LANG] ?: ""
    }

    suspend fun setAppLang(code: String) {
        context.gpucapDataStore.edit { prefs ->
            prefs[KEY_APP_LANG] = code
        }
    }

    /** 将旧版 SharedPreferences 数据无缝迁移到 DataStore */
    suspend fun migrateLegacyIfNeeded() {
        val current = context.gpucapDataStore.data.first()
        if (current[KEY_LAST_URL] != null || current[KEY_HISTORY_JSON] != null) return

        val legacy = context.getSharedPreferences(LEGACY_PREFS, Context.MODE_PRIVATE)
        val oldLast = legacy.getString("last_url", "") ?: ""
        val oldHist = (legacy.getString("history_urls", "") ?: "")
            .split("\n")
            .map { it.trim() }
            .filter { it.isNotEmpty() }

        if (oldLast.isNotEmpty() || oldHist.isNotEmpty()) {
            context.gpucapDataStore.edit { prefs ->
                if (oldLast.isNotEmpty()) prefs[KEY_LAST_URL] = oldLast
                if (oldHist.isNotEmpty()) prefs[KEY_HISTORY_JSON] = json.encodeToString(oldHist)
            }
        }
    }

    suspend fun saveAndRecord(url: String) {
        context.gpucapDataStore.edit { prefs ->
            prefs[KEY_LAST_URL] = url
            val list = decodeHistory(prefs[KEY_HISTORY_JSON]).toMutableList()
            list.remove(url)
            list.add(0, url)
            prefs[KEY_HISTORY_JSON] = json.encodeToString(list.take(8))
        }
    }

    suspend fun addHistoryOnly(url: String) {
        context.gpucapDataStore.edit { prefs ->
            val list = decodeHistory(prefs[KEY_HISTORY_JSON]).toMutableList()
            list.remove(url)
            list.add(0, url)
            prefs[KEY_HISTORY_JSON] = json.encodeToString(list.take(8))
        }
    }

    suspend fun removeHistory(url: String) {
        context.gpucapDataStore.edit { prefs ->
            val list = decodeHistory(prefs[KEY_HISTORY_JSON]).toMutableList()
            list.remove(url)
            prefs[KEY_HISTORY_JSON] = json.encodeToString(list)
        }
    }

    private fun decodeHistory(raw: String?): List<String> {
        if (raw.isNullOrBlank()) return emptyList()
        return runCatching { json.decodeFromString<List<String>>(raw) }.getOrDefault(emptyList())
    }

    /** 规范化并校验用户输入或扫码提取的地址（兼容 IPv4、域名及 [IPv6]） */
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

    /** 使用 kotlinx.serialization 解析二维码载荷 */
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

    /** 使用 OkHttp 探测目标地址的 GPUCap 服务可达性与往返延迟 */
    suspend fun pingUrl(baseUrl: String): PingStatus = withContext(Dispatchers.IO) {
        val t0 = System.nanoTime()
        try {
            val req = Request.Builder()
                .url("$baseUrl/api/state")
                .get()
                .build()
            httpClient.newCall(req).execute().use { resp ->
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
}
