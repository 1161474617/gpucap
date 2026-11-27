package com.gpucap.nativeapp

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

enum class AppPage {
    Connect, Monitor, QrScan, GpuTable
}

enum class MonitorTab(val label: String) {
    Gpu("实时GPU"),
    Cpu("CPU"),
    Trend("趋势"),
    Events("事件"),
    Wer("WER"),
    Alerts("告警"),
}

data class MainUiState(
    val ready: Boolean = false,
    val page: AppPage = AppPage.Connect,
    val currentUrl: String = "",
    val inputUrl: String = "",
    val historyUrls: List<String> = emptyList(),
    val pingMap: Map<String, PingStatus> = emptyMap(),
    val dualChoice: QrParsedResult.Dual? = null,
    val discoveredHosts: List<DiscoveredHost> = emptyList(),
    val discoveringHosts: Boolean = false,
    val keepScreenOn: Boolean = false,
    val bigHudMode: Boolean = false,
    val showPinDialog: Boolean = false,
    val pinError: String? = null,
    val liveAlertBanner: String? = null,
    val lastNotifiedAlertCount: Int = 0,
    // 原生监控数据状态
    val monitorState: StateResponse = StateResponse(),
    val selectedSession: String = "",
    val manualSessionLock: Boolean = false,
    val fullHistory: Boolean = false,
    val paused: Boolean = false,
    val onlyBad: Boolean = false,
    val activeTab: MonitorTab = MonitorTab.Gpu,
    val trendWindow: Int = 20,
    val logFilter: String = "",
    val connError: String? = null,
    val isRefreshing: Boolean = false,
    // 远程设置弹窗
    val showSettings: Boolean = false,
    val settingsConfig: ServerConfigDto = ServerConfigDto(),
    val gpuDevices: List<GpuDeviceDto> = emptyList(),
    // 事故取证
    val incidents: List<IncidentItemDto> = emptyList(),
    val selectedIncident: String = "",
    val previewFile: String = "incident.json",
    val previewText: String = "",
)

/**
 * 原生版主 ViewModel：负责本地存储、并发测速以及原生监控界面的每秒状态轮询与远程控制。
 */
class MainViewModel(application: Application) : AndroidViewModel(application) {

    private val repo = HostRepository(application)

    private val _uiState = MutableStateFlow(MainUiState())
    val uiState: StateFlow<MainUiState> = _uiState.asStateFlow()

    private val _toastFlow = MutableSharedFlow<String>(extraBufferCapacity = 6)
    val toastFlow: SharedFlow<String> = _toastFlow.asSharedFlow()

    private var pollJob: Job? = null

    init {
        viewModelScope.launch {
            val saved = repo.lastUrlFlow.first()
            val history = repo.historyFlow.first()
            val keepOn = repo.keepScreenOnFlow.first()
            repo.authPinFlow.first()
            _uiState.update {
                it.copy(
                    ready = true,
                    page = if (saved.isNotEmpty()) AppPage.Monitor else AppPage.Connect,
                    currentUrl = saved,
                    inputUrl = saved,
                    historyUrls = history,
                    keepScreenOn = keepOn,
                )
            }
            if (saved.isNotEmpty()) {
                startPolling()
            } else {
                pingAll()
                discoverLanHosts()
            }
        }

        viewModelScope.launch {
            repo.historyFlow.collect { list ->
                _uiState.update { it.copy(historyUrls = list) }
            }
        }
    }

    fun onInputChange(value: String) {
        _uiState.update { it.copy(inputUrl = value) }
    }

    fun connectManual() {
        val norm = repo.normalizeUrl(_uiState.value.inputUrl)
        if (norm == null) {
            _toastFlow.tryEmit("请输入有效的 IP 或网址")
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
                    selectedSession = "",
                    fullHistory = false,
                    connError = null,
                    page = AppPage.Monitor,
                )
            }
            startPolling()
        }
    }

    fun switchToConnect() {
        pollJob?.cancel()
        _uiState.update { it.copy(page = AppPage.Connect) }
        pingAll()
    }

    fun openQrScanner() {
        pollJob?.cancel()
        _uiState.update { it.copy(page = AppPage.QrScan) }
    }

    fun closeQrScanner() {
        _uiState.update { it.copy(page = AppPage.Connect) }
    }

    fun openGpuTable() {
        _uiState.update { it.copy(page = AppPage.GpuTable) }
    }

    fun openDetailTab(tab: MonitorTab) {
        _uiState.update { it.copy(activeTab = tab, page = AppPage.GpuTable) }
        if (tab == MonitorTab.Alerts) {
            viewModelScope.launch { loadIncidents() }
        }
    }

    fun closeGpuTable() {
        _uiState.update { it.copy(page = AppPage.Monitor) }
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
                _uiState.update { s -> s.copy(pingMap = s.pingMap + (u to res)) }
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

    // MARK: - 监控轮询与远程控制

    fun startPolling() {
        pollJob?.cancel()
        pollJob = viewModelScope.launch {
            while (isActive && (_uiState.value.page == AppPage.Monitor || _uiState.value.page == AppPage.GpuTable)) {
                if (!_uiState.value.paused) {
                    refreshOnce(manual = false)
                }
                delay(1000L)
            }
        }
    }

    fun manualRefresh() {
        viewModelScope.launch {
            _uiState.update { it.copy(isRefreshing = true, connError = null) }
            refreshOnce(manual = true)
            _uiState.update { it.copy(isRefreshing = false) }
        }
    }

    private suspend fun refreshOnce(manual: Boolean) {
        val st = _uiState.value
        val url = st.currentUrl
        if (url.isEmpty()) return
        val res = repo.fetchState(url, st.selectedSession, st.fullHistory)
        res.onSuccess { data ->
            var needImmediateRefetch = false
            var delayMs = 0L
            _uiState.update { cur ->
                val prevLive = cur.monitorState.liveRunning
                val prevCollector = cur.monitorState.collectorRunning
                val activeSess = data.sessions.firstOrNull { it.active && it.name != "__live__" }?.name

                var nextSess = cur.selectedSession.ifEmpty { data.session }
                var nextLock = cur.manualSessionLock
                var nextPaused = cur.paused

                if ((!prevLive && data.liveRunning) || (!cur.manualSessionLock && data.liveRunning && data.session != "__live__")) {
                    nextSess = "__live__"
                    nextLock = false
                    nextPaused = false
                    if (data.session != "__live__") needImmediateRefetch = true
                } else if (!prevCollector && data.collectorRunning) {
                    nextSess = activeSess ?: ""
                    nextLock = false
                    nextPaused = false
                    if (activeSess == null || data.session != activeSess) {
                        needImmediateRefetch = true
                        if (activeSess == null) delayMs = 800L
                    }
                } else if (!cur.manualSessionLock && data.collectorRunning && activeSess != null && data.session != activeSess) {
                    nextSess = activeSess
                    needImmediateRefetch = true
                }

                val effTab = if (data.cpu == null && cur.activeTab == MonitorTab.Cpu) {
                    MonitorTab.Gpu
                } else {
                    cur.activeTab
                }
                val newAlert = if (data.alertCount > cur.lastNotifiedAlertCount && (cur.lastNotifiedAlertCount > 0 || data.liveRunning)) {
                    data.alerts.lastOrNull()?.let { "[${it.time}] ${it.source}: ${it.message}" } ?: cur.liveAlertBanner
                } else if (data.alertCount == 0) {
                    null
                } else {
                    cur.liveAlertBanner
                }
                cur.copy(
                    monitorState = data,
                    selectedSession = nextSess,
                    manualSessionLock = nextLock,
                    paused = nextPaused,
                    activeTab = effTab,
                    connError = null,
                    liveAlertBanner = newAlert,
                    lastNotifiedAlertCount = data.alertCount,
                )
            }
            if (needImmediateRefetch) {
                if (delayMs > 0L) delay(delayMs)
                val latest = _uiState.value
                repo.fetchState(url, latest.selectedSession, latest.fullHistory).onSuccess { fresh ->
                    _uiState.update { cur ->
                        cur.copy(
                            monitorState = fresh,
                            selectedSession = cur.selectedSession.ifEmpty { fresh.session },
                            connError = null,
                        )
                    }
                }
            }
            if (_uiState.value.activeTab == MonitorTab.Alerts) {
                loadIncidents()
            }
        }.onFailure { err ->
            if (err is AuthRequiredException) {
                _uiState.update { it.copy(showPinDialog = true, connError = null) }
            } else if (manual || _uiState.value.monitorState.gpuRows.isEmpty()) {
                _uiState.update {
                    it.copy(
                        connError = "无法连接到 $url\n(${err.message ?: "网络异常"})",
                    )
                }
            }
        }
    }

    fun setTab(tab: MonitorTab) {
        _uiState.update { it.copy(activeTab = tab) }
        if (tab == MonitorTab.Alerts) {
            viewModelScope.launch { loadIncidents() }
        }
    }

    fun selectSession(name: String) {
        val picked = _uiState.value.monitorState.sessions.firstOrNull { it.name == name }
        val lock = !(name == "__live__" || picked?.active == true)
        _uiState.update { it.copy(selectedSession = name, manualSessionLock = lock, fullHistory = false) }
        manualRefresh()
    }

    fun togglePaused() {
        _uiState.update { it.copy(paused = !it.paused) }
    }

    fun toggleOnlyBad() {
        _uiState.update { it.copy(onlyBad = !it.onlyBad) }
    }

    fun toggleKeepScreenOn() {
        val next = !_uiState.value.keepScreenOn
        _uiState.update { it.copy(keepScreenOn = next) }
        viewModelScope.launch { repo.setKeepScreenOn(next) }
        _toastFlow.tryEmit(if (next) "已开启副屏常亮" else "已关闭副屏常亮")
    }

    fun toggleBigHudMode() {
        val next = !_uiState.value.bigHudMode
        _uiState.update { it.copy(bigHudMode = next) }
    }

    fun submitAuthPin(pin: String) {
        val url = _uiState.value.currentUrl
        if (url.isEmpty()) return
        viewModelScope.launch {
            repo.verifyPin(url, pin)
                .onSuccess {
                    repo.saveAuthPin(pin)
                    _uiState.update { it.copy(showPinDialog = false, pinError = null) }
                    refreshOnce(manual = true)
                }
                .onFailure { err ->
                    _uiState.update { it.copy(pinError = err.message ?: "口令错误") }
                }
        }
    }

    fun dismissPinDialog() {
        _uiState.update { it.copy(showPinDialog = false, pinError = null) }
    }

    fun dismissLiveAlertBanner() {
        _uiState.update { it.copy(liveAlertBanner = null) }
    }

    fun discoverLanHosts() {
        if (_uiState.value.discoveringHosts) return
        _uiState.update { it.copy(discoveringHosts = true) }
        viewModelScope.launch {
            val hosts = repo.discoverLanHosts()
            _uiState.update { it.copy(discoveredHosts = hosts, discoveringHosts = false) }
        }
    }

    fun copyLiveSummary(onCopied: (String) -> Unit) {
        val url = _uiState.value.currentUrl
        if (url.isEmpty()) return
        viewModelScope.launch {
            repo.fetchLiveSummary(url)
                .onSuccess { summary ->
                    onCopied(summary)
                    _toastFlow.tryEmit("已复制 Markdown 诊断摘要")
                }
                .onFailure { _toastFlow.tryEmit(it.message ?: "获取摘要失败") }
        }
    }

    fun saveLiveSession() {
        val url = _uiState.value.currentUrl
        if (url.isEmpty()) return
        viewModelScope.launch {
            repo.saveLiveSession(url)
                .onSuccess { sess ->
                    _toastFlow.tryEmit("已补存为会话：$sess")
                    _uiState.update { it.copy(selectedSession = sess, fullHistory = false) }
                    refreshOnce(manual = true)
                }
                .onFailure { _toastFlow.tryEmit(it.message ?: "保存会话失败") }
        }
    }

    fun loadFullHistory() {
        _uiState.update { it.copy(fullHistory = true) }
        manualRefresh()
    }

    fun setTrendWindow(n: Int) {
        _uiState.update { it.copy(trendWindow = n) }
    }

    fun setLogFilter(q: String) {
        _uiState.update { it.copy(logFilter = q) }
    }

    fun toggleCollector() {
        val st = _uiState.value
        val start = !st.monitorState.collectorRunning
        viewModelScope.launch {
            repo.setCollector(st.currentUrl, start)
                .onSuccess {
                    _toastFlow.tryEmit(if (start) "已发送启动采集指令" else "已发送停止采集指令")
                    if (start) {
                        // 启动采集后会生成新会话，清空当前选中的旧会话以便自动切到最新采集会话
                        _uiState.update { it.copy(selectedSession = "", manualSessionLock = false, fullHistory = false, paused = false) }
                        refreshOnce(manual = false)
                        delay(1200L)
                        _uiState.update { it.copy(selectedSession = "", fullHistory = false) }
                        refreshOnce(manual = false)
                    } else {
                        delay(400L)
                        refreshOnce(manual = false)
                    }
                }
                .onFailure { _toastFlow.tryEmit(it.message ?: "操作失败") }
        }
    }

    fun toggleLiveMonitor() {
        val st = _uiState.value
        val start = !st.monitorState.liveRunning
        viewModelScope.launch {
            repo.setLiveMonitor(st.currentUrl, start)
                .onSuccess {
                    if (start) {
                        _toastFlow.tryEmit("已开启纯内存实时监测（不保存磁盘）")
                        _uiState.update { it.copy(selectedSession = "__live__", manualSessionLock = false, fullHistory = true, paused = false) }
                    } else {
                        _toastFlow.tryEmit("已停止实时监测（保留内存日志）")
                    }
                    delay(250L)
                    refreshOnce(manual = false)
                }
                .onFailure { _toastFlow.tryEmit(it.message ?: "操作失败") }
        }
    }

    fun toggleStartup() {
        val st = _uiState.value
        val next = !st.monitorState.startupEnabled
        viewModelScope.launch {
            repo.setStartup(st.currentUrl, next)
                .onSuccess {
                    _toastFlow.tryEmit(if (next) "已开启开机自启" else "已关闭开机自启")
                    refreshOnce(manual = false)
                }
                .onFailure { _toastFlow.tryEmit(it.message ?: "设置失败") }
        }
    }

    fun deleteCurrentSession() {
        val st = _uiState.value
        val target = st.selectedSession
        if (target.isEmpty()) return
        viewModelScope.launch {
            repo.deleteSession(st.currentUrl, target)
                .onSuccess {
                    _toastFlow.tryEmit("已删除会话 $target")
                    _uiState.update { it.copy(selectedSession = "", fullHistory = false) }
                    refreshOnce(manual = true)
                }
                .onFailure { _toastFlow.tryEmit(it.message ?: "删除失败") }
        }
    }

    fun openSettings() {
        val url = _uiState.value.currentUrl
        viewModelScope.launch {
            repo.fetchConfig(url)
                .onSuccess { res ->
                    _uiState.update {
                        it.copy(
                            showSettings = true,
                            settingsConfig = res.config,
                            gpuDevices = res.gpus,
                        )
                    }
                }
                .onFailure { _toastFlow.tryEmit(it.message ?: "读取电脑端配置失败") }
        }
    }

    fun closeSettings() {
        _uiState.update { it.copy(showSettings = false) }
    }

    fun updateSettingsConfig(cfg: ServerConfigDto) {
        _uiState.update { it.copy(settingsConfig = cfg) }
    }

    fun saveSettings() {
        val st = _uiState.value
        viewModelScope.launch {
            repo.saveConfig(st.currentUrl, st.settingsConfig)
                .onSuccess {
                    _uiState.update { it.copy(showSettings = false) }
                    _toastFlow.tryEmit("设置已保存到电脑端（下次采集生效）")
                }
                .onFailure { _toastFlow.tryEmit(it.message ?: "保存设置失败") }
        }
    }

    private suspend fun loadIncidents() {
        val st = _uiState.value
        repo.fetchIncidents(st.currentUrl, st.selectedSession)
            .onSuccess { items ->
                val curSel = st.selectedIncident.takeIf { sel -> items.any { it.name == sel } }
                    ?: items.firstOrNull()?.name
                    ?: ""
                _uiState.update { it.copy(incidents = items, selectedIncident = curSel) }
                if (curSel.isNotEmpty()) {
                    loadIncidentPreview(curSel, _uiState.value.previewFile)
                }
            }
    }

    fun selectIncident(name: String) {
        _uiState.update { it.copy(selectedIncident = name) }
        viewModelScope.launch { loadIncidentPreview(name, _uiState.value.previewFile) }
    }

    fun selectPreviewFile(file: String) {
        _uiState.update { it.copy(previewFile = file) }
        val inc = _uiState.value.selectedIncident
        if (inc.isNotEmpty()) {
            viewModelScope.launch { loadIncidentPreview(inc, file) }
        }
    }

    private suspend fun loadIncidentPreview(name: String, file: String) {
        repo.fetchIncidentPreview(_uiState.value.currentUrl, name, file)
            .onSuccess { txt -> _uiState.update { it.copy(previewText = txt) } }
            .onFailure { _uiState.update { it.copy(previewText = "(读取失败)") } }
    }

    fun emitToast(msg: String) {
        _toastFlow.tryEmit(msg)
    }

    override fun onCleared() {
        pollJob?.cancel()
        super.onCleared()
    }
}
