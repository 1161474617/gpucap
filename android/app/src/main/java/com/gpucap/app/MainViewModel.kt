package com.gpucap.app

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

enum class AppPage {
    Connect, Web, QrScan
}

data class MainUiState(
    val ready: Boolean = false,
    val isZh: Boolean = false,
    val showLangPicker: Boolean = false,
    val page: AppPage = AppPage.Connect,
    val currentUrl: String = "",
    val inputUrl: String = "",
    val historyUrls: List<String> = emptyList(),
    val pingMap: Map<String, PingStatus> = emptyMap(),
    val dualChoice: QrParsedResult.Dual? = null,
)

/**
 * 遵循单向数据流（UDF）的 ViewModel：
 * 协调 DataStore 响应式存储、并发测速、页面导航与扫码解析。
 */
class MainViewModel(application: Application) : AndroidViewModel(application) {

    private val repo = HostRepository(application)

    private val _uiState = MutableStateFlow(MainUiState())
    val uiState: StateFlow<MainUiState> = _uiState.asStateFlow()

    private val _toastFlow = MutableSharedFlow<String>(extraBufferCapacity = 4)
    val toastFlow: SharedFlow<String> = _toastFlow.asSharedFlow()

    init {
        viewModelScope.launch {
            repo.migrateLegacyIfNeeded()
            val saved = repo.lastUrlFlow.first()
            val history = repo.historyFlow.first()
            val rawLang = repo.appLangRawFlow.first()
            val (initZh, needPicker) = when (rawLang) {
                "zh-CN" -> true to false
                "en" -> false to false
                else -> false to true
            }
            _uiState.update {
                it.copy(
                    ready = true,
                    isZh = initZh,
                    showLangPicker = needPicker,
                    page = if (saved.isNotEmpty()) AppPage.Web else AppPage.Connect,
                    currentUrl = saved,
                    inputUrl = saved,
                    historyUrls = history,
                )
            }
            if (saved.isEmpty()) {
                pingAll()
            }
        }

        // 持续订阅 DataStore 历史列表变化
        viewModelScope.launch {
            repo.historyFlow.collect { list ->
                _uiState.update { it.copy(historyUrls = list) }
            }
        }
    }

    private fun tr(zh: String, en: String): String = if (_uiState.value.isZh) zh else en

    fun setAppLang(zh: Boolean) {
        _uiState.update { it.copy(isZh = zh, showLangPicker = false) }
        viewModelScope.launch { repo.setAppLang(if (zh) "zh-CN" else "en") }
    }

    fun onInputChange(value: String) {
        _uiState.update { it.copy(inputUrl = value) }
    }

    fun connectManual() {
        val norm = repo.normalizeUrl(_uiState.value.inputUrl)
        if (norm == null) {
            _toastFlow.tryEmit(tr("请输入有效的 IP 或网址", "Please enter a valid IP or URL"))
            return
        }
        openAndSave(norm)
    }

    fun openAndSave(url: String) {
        viewModelScope.launch {
            repo.saveAndRecord(url)
            _uiState.update {
                it.copy(
                    currentUrl = url,
                    inputUrl = url,
                    dualChoice = null,
                    page = AppPage.Web,
                )
            }
        }
    }

    fun switchToConnect() {
        _uiState.update { it.copy(page = AppPage.Connect) }
        pingAll()
    }

    fun openQrScanner() {
        _uiState.update { it.copy(page = AppPage.QrScan) }
    }

    fun closeQrScanner() {
        _uiState.update { it.copy(page = AppPage.Connect) }
    }

    fun dismissDualChoice() {
        _uiState.update { it.copy(dualChoice = null) }
    }

    fun removeHistory(url: String) {
        viewModelScope.launch {
            repo.removeHistory(url)
            _uiState.update { s -> s.copy(pingMap = s.pingMap - url) }
        }
    }

    fun pingAll() {
        val urls = _uiState.value.historyUrls
        if (urls.isEmpty()) return
        _uiState.update { s ->
            val next = s.pingMap.toMutableMap()
            urls.forEach { next[it] = PingStatus.Checking }
            s.copy(pingMap = next)
        }
        urls.forEach { u ->
            viewModelScope.launch {
                val res = repo.pingUrl(u)
                _uiState.update { s ->
                    s.copy(pingMap = s.pingMap + (u to res))
                }
            }
        }
    }

    fun handleQrDetected(raw: String) {
        _uiState.update { it.copy(page = AppPage.Connect) }
        when (val parsed = repo.parseQrPayload(raw)) {
            is QrParsedResult.Single -> openAndSave(parsed.url)
            is QrParsedResult.Dual -> {
                viewModelScope.launch {
                    repo.addHistoryOnly(parsed.ipv6)
                    repo.addHistoryOnly(parsed.ipv4)
                    _uiState.update { it.copy(dualChoice = parsed) }
                }
            }
            QrParsedResult.Invalid -> _toastFlow.tryEmit("二维码中未包含有效的 GPUCap 主机地址")
        }
    }

    fun emitToast(msg: String) {
        _toastFlow.tryEmit(msg)
    }
}
