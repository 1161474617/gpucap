package com.gpucap.nativeapp

import androidx.compose.runtime.staticCompositionLocalOf

/** 应用显示语言枚举 */
enum class AppLang(val code: String) {
    EN("en"),
    ZH_CN("zh-CN"),
}

/**
 * 轻量双语词典（KISS）：
 * 集中管理核心常用文案，并通过 `t(zh, en)` 便捷方法支持就地双语切换。
 */
data class AppStrings(
    val isZh: Boolean,
    val appTitle: String,
    val heroSubtitle: String,
    val connectHostTitle: String,
    val inputHint: String,
    val saveAndConnect: String,
    val scanQrConnect: String,
    val lanDiscovery: String,
    val scanningLan: String,
    val noLanHosts: String,
    val recentHosts: String,
    val noRecentHosts: String,
    val pingAll: String,
    val remove: String,
    val switchHost: String,
    val startCap: String,
    val stopCap: String,
    val pauseUi: String,
    val resumeUi: String,
    val settings: String,
    val fullDetails: String,
    val tabGpu: String,
    val tabCpu: String,
    val tabTrend: String,
    val tabEvents: String,
    val tabWer: String,
    val tabAlerts: String,
    val statusIdle: String,
    val statusRunning: String,
    val statusOffline: String,
    val saveBtn: String,
    val cancelBtn: String,
    val closeBtn: String,
) {
    fun t(zh: String, en: String): String = if (isZh) zh else en

    fun tabLabel(tab: MonitorTab): String = when (tab) {
        MonitorTab.Gpu -> tabGpu
        MonitorTab.Cpu -> tabCpu
        MonitorTab.Trend -> tabTrend
        MonitorTab.Events -> tabEvents
        MonitorTab.Wer -> tabWer
        MonitorTab.Alerts -> tabAlerts
    }

    fun trThr(text: String): String {
        if (isZh) return text
        return when {
            text == "正常" -> "Normal"
            text.startsWith("异常:") -> "Alert: ${text.removePrefix("异常:")}"
            else -> text
        }
    }

    /** 将 Windows 服务端下发的中文状态/摘要字段转换为当前界面语言 */
    fun trBackend(text: String): String {
        if (isZh || text.isEmpty()) return text
        return text
            .replace("● 实时监测中（仅内存日志）", "● Live Monitor (Memory)")
            .replace("● 实时监测已停止（保留内存日志）", "● Live Monitor Stopped")
            .replace("● 未在采集（暂无会话）", "● Idle (No Session)")
            .replace("● 未在采集（查看历史）", "● Idle (History)")
            .replace("● 采集器运行中", "● Collector Running")
            .replace("● 采集器初始化中...", "● Initializing...")
            .replace("● 心跳延迟 ", "● Heartbeat Lag ")
            .replace("● 无心跳（采集器未运行）", "● No Heartbeat")
            .replace("内存 ", "RAM ")
            .replace("可用 ", "Avail ")
            .replace("非分页 ", "NonPaged ")
            .replace("游戏 ", "Game ")
            .replace("未运行", "Not Running")
            .replace("仅内存", "Memory")
            .replace("正常", "OK")
            .replace("异常", "Alert")
    }
}

val zhStrings = AppStrings(
    isZh = true,
    appTitle = "GPUCap 无线监控",
    heroSubtitle = "局域网自动发现 · 实时采集与告警取证",
    connectHostTitle = "连接电脑端主机",
    inputHint = "输入 IP 或网址，例如 192.168.1.100:8787",
    saveAndConnect = "保存并连接",
    scanQrConnect = "扫码连接",
    lanDiscovery = "局域网主机发现",
    scanningLan = "正在扫描局域网...",
    noLanHosts = "未发现局域网主机，点击右侧刷新扫描",
    recentHosts = "历史连接记录",
    noRecentHosts = "暂无历史记录",
    pingAll = "全部测速",
    remove = "移除",
    switchHost = "切换",
    startCap = "开始",
    stopCap = "停止",
    pauseUi = "暂停刷新",
    resumeUi = "恢复刷新",
    settings = "设置",
    fullDetails = "全屏明细",
    tabGpu = "实时GPU",
    tabCpu = "CPU",
    tabTrend = "趋势",
    tabEvents = "事件",
    tabWer = "WER",
    tabAlerts = "告警",
    statusIdle = "待命",
    statusRunning = "采集中",
    statusOffline = "离线",
    saveBtn = "保存",
    cancelBtn = "取消",
    closeBtn = "关闭",
)

val enStrings = AppStrings(
    isZh = false,
    appTitle = "GPUCap Wireless Monitor",
    heroSubtitle = "LAN Discovery · Live Telemetry & Incident Forensics",
    connectHostTitle = "Connect to Host",
    inputHint = "Enter IP or URL, e.g. 192.168.1.100:8787",
    saveAndConnect = "Save & Connect",
    scanQrConnect = "Scan QR Code",
    lanDiscovery = "LAN Host Discovery",
    scanningLan = "Scanning LAN...",
    noLanHosts = "No LAN hosts found. Tap refresh to scan.",
    recentHosts = "Recent Hosts",
    noRecentHosts = "No recent hosts",
    pingAll = "Ping All",
    remove = "Remove",
    switchHost = "Switch",
    startCap = "Start",
    stopCap = "Stop",
    pauseUi = "Pause",
    resumeUi = "Resume",
    settings = "Settings",
    fullDetails = "Full View",
    tabGpu = "GPU",
    tabCpu = "CPU",
    tabTrend = "Trend",
    tabEvents = "Events",
    tabWer = "WER",
    tabAlerts = "Alerts",
    statusIdle = "Idle",
    statusRunning = "Running",
    statusOffline = "Offline",
    saveBtn = "Save",
    cancelBtn = "Cancel",
    closeBtn = "Close",
)

fun stringsFor(lang: AppLang): AppStrings = if (lang == AppLang.ZH_CN) zhStrings else enStrings

val LocalStrings = staticCompositionLocalOf { enStrings }
