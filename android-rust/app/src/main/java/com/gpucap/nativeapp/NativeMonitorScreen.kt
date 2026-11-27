package com.gpucap.nativeapp

import android.app.DownloadManager
import android.content.Context
import android.net.Uri
import android.os.Environment
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items as gridItems
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowRight
import androidx.compose.material.icons.rounded.BugReport
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.DeleteOutline
import androidx.compose.material.icons.rounded.Download
import androidx.compose.material.icons.rounded.EventNote
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material.icons.rounded.Memory
import androidx.compose.material.icons.rounded.OpenInFull
import androidx.compose.material.icons.rounded.Pause
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Save
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.Visibility
import androidx.compose.material.icons.rounded.VisibilityOff
import androidx.compose.material.icons.rounded.ShowChart
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material.icons.rounded.SwapHoriz
import androidx.compose.material.icons.rounded.TableChart
import androidx.compose.material.icons.rounded.WarningAmber
import androidx.compose.material.icons.rounded.WifiOff
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import java.net.URLEncoder
import java.util.Locale

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NativeMonitorScreen(
    uiState: MainUiState,
    vm: MainViewModel,
) {
    val s = LocalStrings.current
    val context = LocalContext.current
    val data = uiState.monitorState
    val latestGpu = data.gpuRows.lastOrNull()
    var deleteConfirm by remember { mutableStateOf(false) }
    var splitDetailRow by remember { mutableStateOf<GpuRowDto?>(null) }
    var splitLogDetail by remember { mutableStateOf<LogEntryDto?>(null) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(GpuColors.Bg)
            .statusBarsPadding()
            .navigationBarsPadding(),
    ) {
        // 顶部原生控制栏：宽屏/平板横屏单行合并，窄屏紧凑两行
        BoxWithConstraints(
            modifier = Modifier
                .fillMaxWidth()
                .background(GpuColors.TopBar)
                .padding(horizontal = 10.dp, vertical = 5.dp),
        ) {
            val singleRowTop = maxWidth >= 700.dp
            val collectorBtn: @Composable () -> Unit = {
                Surface(
                    onClick = vm::toggleCollector,
                    shape = RoundedCornerShape(8.dp),
                    color = if (data.collectorRunning) GpuColors.BadBg else GpuColors.Emerald,
                    border = if (data.collectorRunning) BorderStroke(1.dp, GpuColors.BadBorder) else null,
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.padding(horizontal = 9.dp, vertical = 5.dp),
                    ) {
                        Icon(
                            imageVector = if (data.collectorRunning) Icons.Rounded.Stop else Icons.Rounded.PlayArrow,
                            contentDescription = null,
                            tint = if (data.collectorRunning) GpuColors.BadLight else GpuColors.EmeraldText,
                            modifier = Modifier.size(14.5.dp),
                        )
                        Spacer(modifier = Modifier.width(3.dp))
                        Text(
                            text = if (data.collectorRunning) s.t("停止采集", "Stop Cap") else s.t("启动采集", "Start Cap"),
                            color = if (data.collectorRunning) GpuColors.BadLight else GpuColors.EmeraldText,
                            fontSize = 11.5.sp,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }
            }

            val liveBtn: @Composable () -> Unit = {
                Surface(
                    onClick = vm::toggleLiveMonitor,
                    shape = RoundedCornerShape(8.dp),
                    color = if (data.liveRunning) GpuColors.WarnBg else GpuColors.AccentBg,
                    border = BorderStroke(
                        1.dp,
                        if (data.liveRunning) GpuColors.WarnBorder else GpuColors.AccentBorder,
                    ),
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.padding(horizontal = 9.dp, vertical = 5.dp),
                    ) {
                        Icon(
                            imageVector = if (data.liveRunning) Icons.Rounded.Stop else Icons.Rounded.PlayArrow,
                            contentDescription = null,
                            tint = if (data.liveRunning) GpuColors.WarnLight else GpuColors.Accent,
                            modifier = Modifier.size(14.5.dp),
                        )
                        Spacer(modifier = Modifier.width(3.dp))
                        Text(
                            text = if (data.liveRunning) s.t("停止监测", "Stop Live") else s.t("实时监测", "Live Mon"),
                            color = if (data.liveRunning) GpuColors.WarnLight else GpuColors.Accent,
                            fontSize = 11.5.sp,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }
            }

            val hbColor = when (data.heartbeat.level) {
                "ok" -> GpuColors.EmeraldLight
                "warn" -> GpuColors.WarnLight
                "bad" -> GpuColors.BadLight
                else -> GpuColors.Dim
            }
            val heartbeatPill: @Composable (Modifier) -> Unit = { m ->
                Surface(
                    shape = RoundedCornerShape(99.dp),
                    color = GpuColors.Card,
                    border = BorderStroke(1.dp, GpuColors.Border),
                    modifier = m,
                ) {
                    Text(
                        text = s.trBackend(data.heartbeat.text),
                        color = hbColor,
                        fontSize = 11.5.sp,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
                    )
                }
            }

            val alertBadge: @Composable () -> Unit = {
                if (data.alertCount > 0) {
                    Surface(
                        onClick = { vm.openDetailTab(MonitorTab.Alerts) },
                        shape = RoundedCornerShape(8.dp),
                        color = GpuColors.WarnBg,
                        border = BorderStroke(1.dp, GpuColors.WarnBorder),
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 5.dp),
                        ) {
                            Icon(
                                imageVector = Icons.Rounded.WarningAmber,
                                contentDescription = null,
                                tint = GpuColors.WarnLight,
                                modifier = Modifier.size(14.dp),
                            )
                            Spacer(modifier = Modifier.width(3.dp))
                            Text(
                                text = s.t("告警 ${data.alertCount}", "Alerts ${data.alertCount}"),
                                color = GpuColors.WarnLight,
                                fontSize = 11.5.sp,
                                fontWeight = FontWeight.SemiBold,
                            )
                        }
                    }
                }
            }

            if (singleRowTop) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Surface(
                        shape = RoundedCornerShape(8.dp),
                        color = GpuColors.AccentBg,
                        border = BorderStroke(1.dp, GpuColors.AccentBorder),
                        modifier = Modifier.size(28.dp),
                    ) {
                        Box(contentAlignment = Alignment.Center) {
                            Icon(
                                imageVector = Icons.Rounded.Memory,
                                contentDescription = null,
                                tint = GpuColors.Accent,
                                modifier = Modifier.size(16.dp),
                            )
                        }
                    }
                    Text(
                        text = "GPUCap",
                        color = Color.White,
                        fontSize = 15.sp,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        text = uiState.currentUrl.removePrefix("http://").removePrefix("https://"),
                        color = GpuColors.Dim,
                        fontSize = 11.5.sp,
                        fontFamily = FontFamily.Monospace,
                        maxLines = 1,
                        overflow = TextOverflow.MiddleEllipsis,
                    )
                    collectorBtn()
                    liveBtn()
                    heartbeatPill(Modifier)
                    Spacer(modifier = Modifier.weight(1f))
                    SmallActionChip(
                        icon = Icons.Rounded.OpenInFull,
                        label = s.t("大字", "HUD"),
                        fg = if (uiState.bigHudMode) GpuColors.Accent else GpuColors.Dim,
                        bg = if (uiState.bigHudMode) GpuColors.AccentBg else GpuColors.SurfaceBtn,
                        border = if (uiState.bigHudMode) GpuColors.AccentBorder else GpuColors.Border,
                        onClick = vm::toggleBigHudMode,
                    )
                    SmallActionChip(
                        icon = if (uiState.keepScreenOn) Icons.Rounded.Visibility else Icons.Rounded.VisibilityOff,
                        label = s.t("常亮", "Awake"),
                        fg = if (uiState.keepScreenOn) GpuColors.EmeraldLight else GpuColors.Dim,
                        bg = if (uiState.keepScreenOn) GpuColors.EmeraldBg else GpuColors.SurfaceBtn,
                        border = if (uiState.keepScreenOn) GpuColors.EmeraldBorder else GpuColors.Border,
                        onClick = vm::toggleKeepScreenOn,
                    )
                    alertBadge()
                    SmallActionChip(
                        icon = Icons.Rounded.Settings,
                        label = null,
                        fg = GpuColors.Text,
                        bg = GpuColors.SurfaceBtn,
                        onClick = vm::openSettings,
                    )
                    SmallActionChip(
                        icon = Icons.Rounded.SwapHoriz,
                        label = s.switchHost,
                        fg = GpuColors.Accent,
                        bg = GpuColors.AccentBg,
                        border = GpuColors.AccentBorder,
                        onClick = vm::switchToConnect,
                    )
                }
            } else {
                Column(verticalArrangement = Arrangement.spacedBy(5.dp)) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Surface(
                            shape = RoundedCornerShape(8.dp),
                            color = GpuColors.AccentBg,
                            border = BorderStroke(1.dp, GpuColors.AccentBorder),
                            modifier = Modifier.size(28.dp),
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    imageVector = Icons.Rounded.Memory,
                                    contentDescription = null,
                                    tint = GpuColors.Accent,
                                    modifier = Modifier.size(16.dp),
                                )
                            }
                        }

                        Text(
                            text = "GPUCap",
                            color = Color.White,
                            fontSize = 15.sp,
                            fontWeight = FontWeight.Bold,
                        )

                        Text(
                            text = uiState.currentUrl.removePrefix("http://").removePrefix("https://"),
                            color = GpuColors.Dim,
                            fontSize = 11.5.sp,
                            fontFamily = FontFamily.Monospace,
                            maxLines = 1,
                            overflow = TextOverflow.MiddleEllipsis,
                            modifier = Modifier.weight(1f),
                        )

                        SmallActionChip(
                            icon = Icons.Rounded.Settings,
                            label = null,
                            fg = GpuColors.Text,
                            bg = GpuColors.SurfaceBtn,
                            onClick = vm::openSettings,
                        )

                        SmallActionChip(
                            icon = Icons.Rounded.SwapHoriz,
                            label = s.switchHost,
                            fg = GpuColors.Accent,
                            bg = GpuColors.AccentBg,
                            border = GpuColors.AccentBorder,
                            onClick = vm::switchToConnect,
                        )
                    }

                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        collectorBtn()
                        liveBtn()
                        heartbeatPill(Modifier.weight(1f))
                        alertBadge()
                    }
                }
            }
        }

        // 主内容区（支持下拉刷新与断线引导）
        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            if (uiState.connError != null && data.gpuRows.isEmpty()) {
                Box(
                    contentAlignment = Alignment.Center,
                    modifier = Modifier.fillMaxSize().padding(24.dp),
                ) {
                    SectionCard {
                        Column(
                            horizontalAlignment = Alignment.CenterHorizontally,
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Icon(
                                imageVector = Icons.Rounded.WifiOff,
                                contentDescription = null,
                                tint = GpuColors.BadLight,
                                modifier = Modifier.size(34.dp),
                            )
                            Spacer(modifier = Modifier.height(10.dp))
                            Text(
                                text = s.t("无法连接到 GPUCap 服务", "Unable to Connect to GPUCap"),
                                color = GpuColors.BadLight,
                                fontSize = 16.sp,
                                fontWeight = FontWeight.Bold,
                            )
                            Spacer(modifier = Modifier.height(8.dp))
                            Text(
                                text = uiState.connError,
                                color = GpuColors.Dim,
                                fontSize = 12.5.sp,
                                textAlign = TextAlign.Center,
                            )
                            Spacer(modifier = Modifier.height(16.dp))
                            Button(
                                onClick = vm::manualRefresh,
                                shape = RoundedCornerShape(12.dp),
                                colors = ButtonDefaults.buttonColors(
                                    containerColor = GpuColors.Emerald,
                                    contentColor = GpuColors.EmeraldText,
                                ),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Text(s.t("重新连接", "Reconnect"), fontWeight = FontWeight.Bold)
                            }
                            Spacer(modifier = Modifier.height(8.dp))
                            OutlinedButton(
                                onClick = vm::switchToConnect,
                                shape = RoundedCornerShape(12.dp),
                                border = BorderStroke(1.dp, GpuColors.Accent),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Text(s.t("返回切换主机", "Back to Host Selection"), color = GpuColors.Accent)
                            }
                        }
                    }
                }
            } else {
                PullToRefreshBox(
                    isRefreshing = uiState.isRefreshing,
                    onRefresh = vm::manualRefresh,
                    modifier = Modifier.fillMaxSize(),
                ) {
                    Column(
                        modifier = Modifier
                            .fillMaxSize()
                            .padding(horizontal = 10.dp, vertical = 6.dp),
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        // 0. 实时告警横幅提醒
                        uiState.liveAlertBanner?.let { banner ->
                            Surface(
                                shape = RoundedCornerShape(10.dp),
                                color = GpuColors.BadBg,
                                border = BorderStroke(1.dp, GpuColors.BadBorder),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                                ) {
                                    Text(
                                        text = s.t("实时告警：$banner", "Live Alert: $banner"),
                                        color = GpuColors.BadLight,
                                        fontSize = 11.5.sp,
                                        fontWeight = FontWeight.SemiBold,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis,
                                        modifier = Modifier.weight(1f),
                                    )
                                    Text(
                                        text = s.closeBtn,
                                        color = Color.White,
                                        fontSize = 11.sp,
                                        fontWeight = FontWeight.Bold,
                                        modifier = Modifier.clickable(onClick = vm::dismissLiveAlertBanner),
                                    )
                                }
                            }
                        }

                        // 0.5 环境指纹摘要条
                        data.env?.let { env ->
                            Surface(
                                shape = RoundedCornerShape(8.dp),
                                color = GpuColors.Card.copy(alpha = 0.85f),
                                border = BorderStroke(1.dp, GpuColors.Border),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                val drv = env.driverVersion.ifBlank { "—" }
                                val chg = env.driverChangedFrom?.let { s.t(" (变更自 $it)", " (from $it)") } ?: ""
                                Text(
                                    text = s.t(
                                        "${env.gpuName} · 驱动 $drv$chg · ${env.pcieMax} · 电源 ${env.powerPlan}",
                                        "${env.gpuName} · Driver $drv$chg · ${env.pcieMax} · Power ${env.powerPlan}",
                                    ),
                                    color = if (env.driverChangedFrom != null) GpuColors.WarnLight else GpuColors.Dim,
                                    fontSize = 10.5.sp,
                                    fontFamily = FontFamily.Monospace,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
                                )
                            }
                        }

                        // 1. 若开启大字模式则全屏展示远距离副屏仪表盘
                        if (uiState.bigHudMode) {
                            BigHudDashboard(
                                data = data,
                                latestGpu = latestGpu,
                                onExit = vm::toggleBigHudMode,
                            )
                        } else {
                        // 1. 动态 KPI 卡片区（平板横屏 1x4，手机 2x2）
                        KpiGrid(
                            latestGpu = latestGpu,
                            data = data,
                            deltaWarn = uiState.settingsConfig.hotspotDeltaWarn,
                            onToggleStartup = vm::toggleStartup,
                        )

                        // 2. 动态主区域：平板横屏(>=840dp)将 SessionToolbar 融入左栏，右栏顶满剩余高度；窄屏保持独立单行工具栏
                        BoxWithConstraints(
                            modifier = Modifier
                                .weight(1f)
                                .fillMaxWidth(),
                        ) {
                            val isSplit = maxWidth >= 840.dp
                            val isMediumGrid = maxWidth in 600.dp..839.dp

                            val sessionBar: @Composable (Boolean) -> Unit = { wrap ->
                                SessionToolbar(
                                    uiState = uiState,
                                    wrapMode = wrap,
                                    onSelectSession = vm::selectSession,
                                    onTogglePause = vm::togglePaused,
                                    onToggleOnlyBad = vm::toggleOnlyBad,
                                    onLoadFull = vm::loadFullHistory,
                                    onExport = {
                                        val sess = uiState.selectedSession
                                        if (sess.isEmpty()) {
                                            vm.emitToast(s.t("当前没有可导出的会话", "No session selected to export"))
                                        } else {
                                            exportZipViaDownloadManager(
                                                context = context,
                                                baseUrl = uiState.currentUrl,
                                                session = sess,
                                                strings = s,
                                                onToast = vm::emitToast,
                                            )
                                        }
                                    },
                                    onRequestDelete = { deleteConfirm = true },
                                )
                            }

                            val onCardClick: (MonitorTab) -> Unit = { tab ->
                                if (isSplit) {
                                    vm.setTab(tab)
                                } else {
                                    vm.openDetailTab(tab)
                                }
                            }

                            val cardsList: List<@Composable () -> Unit> = buildList {
                                add {
                                    DetailEntryCard(
                                        icon = Icons.Rounded.TableChart,
                                        accent = GpuColors.Accent,
                                        title = s.t("实时 GPU 采样表", "Live GPU Telemetry"),
                                        subtitle = latestGpu?.let {
                                            s.t(
                                                "最新 ${it.ts} · 温度 ${it.temp}°C · 功耗 ${it.power}W · 占用 ${it.util}%",
                                                "Latest ${it.ts} · Temp ${it.temp}°C · Power ${it.power}W · Util ${it.util}%",
                                            )
                                        } ?: s.t("固定时间列宽表，点击行查看完整 13 项指标", "Fixed time-column table, tap row for 13 metrics"),
                                        badge = s.t("${data.gpuRows.size} 条", "${data.gpuRows.size} rows"),
                                        selected = isSplit && uiState.activeTab == MonitorTab.Gpu,
                                        compact = isSplit,
                                        onClick = { onCardClick(MonitorTab.Gpu) },
                                    )
                                }
                                data.cpu?.let { cpu ->
                                    add {
                                        val tStr = cpu.tempC?.let { String.format(Locale.US, "%.1f°C", it) } ?: "—"
                                        DetailEntryCard(
                                            icon = Icons.Rounded.Memory,
                                            accent = Color(0xFF38BDF8),
                                            title = s.t("CPU 实时监测", "CPU Live Monitor"),
                                            subtitle = s.t(
                                                "温度 $tStr · 总占用 ${String.format(Locale.US, "%.1f", cpu.totalUsage)}% · ${cpu.avgFreqMhz}MHz",
                                                "Temp $tStr · Usage ${String.format(Locale.US, "%.1f", cpu.totalUsage)}% · ${cpu.avgFreqMhz}MHz",
                                            ),
                                            badge = s.t("${cpu.logicalCores} 核", "${cpu.logicalCores}C"),
                                            selected = isSplit && uiState.activeTab == MonitorTab.Cpu,
                                            compact = isSplit,
                                            onClick = { onCardClick(MonitorTab.Cpu) },
                                        )
                                    }
                                }
                                add {
                                    DetailEntryCard(
                                        icon = Icons.Rounded.ShowChart,
                                        accent = GpuColors.Violet,
                                        title = s.t("趋势曲线", "Trend Charts"),
                                        subtitle = s.t(
                                            "温度 / 热点 / 温差 / 功耗 / 频率 / 内存等 12 项折线图",
                                            "12 telemetry charts: Temp / Hotspot / Delta / Power / Clocks / RAM",
                                        ),
                                        badge = s.t("${uiState.trendWindow} 点窗口", "${uiState.trendWindow} pts"),
                                        selected = isSplit && uiState.activeTab == MonitorTab.Trend,
                                        compact = isSplit,
                                        onClick = { onCardClick(MonitorTab.Trend) },
                                    )
                                }
                                add {
                                    DetailEntryCard(
                                        icon = Icons.Rounded.EventNote,
                                        accent = GpuColors.EmeraldLight,
                                        title = s.t("系统事件", "System Events"),
                                        subtitle = data.events.lastOrNull()?.let {
                                            "${it.time} · ${it.source}：${it.message}"
                                        } ?: s.t("支持关键字过滤与完整事件详情查看", "Keyword filter and full event inspection"),
                                        badge = s.t("${data.events.size} 条", "${data.events.size}"),
                                        selected = isSplit && uiState.activeTab == MonitorTab.Events,
                                        compact = isSplit,
                                        onClick = { onCardClick(MonitorTab.Events) },
                                    )
                                }
                                add {
                                    DetailEntryCard(
                                        icon = Icons.Rounded.BugReport,
                                        accent = GpuColors.Orange,
                                        title = s.t("WER 崩溃与转储", "WER & Crash Dumps"),
                                        subtitle = data.wer.lastOrNull()?.let {
                                            "${it.time} · ${it.message}"
                                        } ?: s.t("监测 Windows 错误报告与应用程序崩溃转储", "Windows Error Reporting and crash dumps"),
                                        badge = s.t("${data.wer.size} 条", "${data.wer.size}"),
                                        selected = isSplit && uiState.activeTab == MonitorTab.Wer,
                                        compact = isSplit,
                                        onClick = { onCardClick(MonitorTab.Wer) },
                                    )
                                }
                                add {
                                    DetailEntryCard(
                                        icon = Icons.Rounded.WarningAmber,
                                        accent = if (data.alertCount > 0) GpuColors.WarnLight else GpuColors.Dim,
                                        title = s.t("告警与事故取证", "Alerts & Forensics"),
                                        subtitle = s.t("阈值告警记录、事故取证包及摘要/日志预览", "Threshold alerts, incident bundles & logs"),
                                        badge = s.t("${data.alerts.size} 告警", "${data.alerts.size}"),
                                        selected = isSplit && uiState.activeTab == MonitorTab.Alerts,
                                        compact = isSplit,
                                        onClick = { onCardClick(MonitorTab.Alerts) },
                                    )
                                }
                            }

                            val headerRow: @Composable () -> Unit = {
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.SpaceBetween,
                                    modifier = Modifier.fillMaxWidth(),
                                ) {
                                    Text(
                                        text = s.t("采集数据明细", "Capture Telemetry"),
                                        color = GpuColors.Dim,
                                        fontSize = 11.sp,
                                        fontWeight = FontWeight.Medium,
                                    )
                                    Surface(
                                        onClick = { vm.openDetailTab(uiState.activeTab) },
                                        shape = RoundedCornerShape(6.dp),
                                        color = GpuColors.AccentBg,
                                        border = BorderStroke(1.dp, GpuColors.AccentBorder),
                                    ) {
                                        Row(
                                            verticalAlignment = Alignment.CenterVertically,
                                            modifier = Modifier.padding(horizontal = 7.dp, vertical = 3.dp),
                                        ) {
                                            Icon(
                                                imageVector = Icons.Rounded.OpenInFull,
                                                contentDescription = null,
                                                tint = GpuColors.Accent,
                                                modifier = Modifier.size(11.dp),
                                            )
                                            Spacer(modifier = Modifier.width(3.dp))
                                            Text(
                                                text = s.t("全屏分页", "Fullscreen"),
                                                color = GpuColors.Accent,
                                                fontSize = 10.5.sp,
                                                fontWeight = FontWeight.SemiBold,
                                            )
                                        }
                                    }
                                }
                            }

                            if (isSplit) {
                                Row(
                                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                                    modifier = Modifier.fillMaxSize(),
                                ) {
                                    Column(
                                        modifier = Modifier
                                            .width(252.dp)
                                            .fillMaxHeight(),
                                        verticalArrangement = Arrangement.spacedBy(8.dp),
                                    ) {
                                        sessionBar(true)

                                        Surface(
                                            shape = RoundedCornerShape(12.dp),
                                            color = GpuColors.Card,
                                            border = BorderStroke(1.dp, GpuColors.Border),
                                            modifier = Modifier
                                                .weight(1f)
                                                .fillMaxWidth(),
                                        ) {
                                            Column(
                                                modifier = Modifier
                                                    .fillMaxSize()
                                                    .verticalScroll(rememberScrollState())
                                                    .padding(9.dp),
                                                verticalArrangement = Arrangement.spacedBy(6.dp),
                                            ) {
                                                headerRow()
                                                cardsList.forEach { it() }
                                            }
                                        }
                                    }

                                    Box(
                                        modifier = Modifier
                                            .weight(1f)
                                            .fillMaxHeight(),
                                    ) {
                                        when (uiState.activeTab) {
                                            MonitorTab.Gpu -> FullGpuTableContent(
                                                rows = data.gpuRows,
                                                onlyBad = uiState.onlyBad,
                                                stats = data.stats,
                                                onClickRow = { splitDetailRow = it },
                                            )
                                            MonitorTab.Cpu -> {
                                                val ctx = androidx.compose.ui.platform.LocalContext.current
                                                CpuMonitorTab(
                                                    cpu = data.cpu,
                                                    cpuPoints = data.cpuPoints,
                                                    liveRunning = data.liveRunning,
                                                    onCopySummary = {
                                                        vm.copyLiveSummary { text ->
                                                            val cm = ctx.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as? android.content.ClipboardManager
                                                            cm?.setPrimaryClip(android.content.ClipData.newPlainText("GPUCap Summary", text))
                                                        }
                                                    },
                                                    onSaveSession = vm::saveLiveSession,
                                                )
                                            }
                                            MonitorTab.Trend -> TrendTab(
                                                gpuRows = data.gpuRows,
                                                memPoints = data.memPoints,
                                                windowSize = uiState.trendWindow,
                                                onChangeWindow = vm::setTrendWindow,
                                            )
                                            MonitorTab.Events -> LogListTab(
                                                entries = data.events,
                                                onlyBad = uiState.onlyBad,
                                                filter = uiState.logFilter,
                                                onFilterChange = vm::setLogFilter,
                                                emptyHint = "暂无系统事件记录",
                                                onClickItem = { splitLogDetail = it },
                                            )
                                            MonitorTab.Wer -> LogListTab(
                                                entries = data.wer,
                                                onlyBad = false,
                                                filter = uiState.logFilter,
                                                onFilterChange = vm::setLogFilter,
                                                emptyHint = "暂未监测到 WER 崩溃与转储记录",
                                                onClickItem = { splitLogDetail = it },
                                            )
                                            MonitorTab.Alerts -> AlertsAndIncidentsTab(
                                                uiState = uiState,
                                                vm = vm,
                                                onClickLog = { splitLogDetail = it },
                                            )
                                        }
                                    }
                                }
                            } else if (isMediumGrid) {
                                Column(
                                    modifier = Modifier
                                        .fillMaxSize()
                                        .verticalScroll(rememberScrollState()),
                                    verticalArrangement = Arrangement.spacedBy(6.dp),
                                ) {
                                    sessionBar(false)
                                    headerRow()
                                    cardsList.chunked(2).forEach { pair ->
                                        Row(
                                            horizontalArrangement = Arrangement.spacedBy(6.dp),
                                            modifier = Modifier.fillMaxWidth(),
                                        ) {
                                            Box(modifier = Modifier.weight(1f)) { pair[0]() }
                                            if (pair.size > 1) {
                                                Box(modifier = Modifier.weight(1f)) { pair[1]() }
                                            } else {
                                                Spacer(modifier = Modifier.weight(1f))
                                            }
                                        }
                                    }
                                }
                            } else {
                                Column(
                                    modifier = Modifier
                                        .fillMaxSize()
                                        .verticalScroll(rememberScrollState()),
                                    verticalArrangement = Arrangement.spacedBy(5.dp),
                                ) {
                                    sessionBar(false)
                                    headerRow()
                                    cardsList.forEach { it() }
                                }
                            }
                        }
                        }
                    }
                }
            }
        }
    }

    // 删除当前会话确认
    if (deleteConfirm) {
        AlertDialog(
            onDismissRequest = { deleteConfirm = false },
            containerColor = GpuColors.Card,
            title = { Text(s.t("确认删除会话", "Delete Session"), color = Color.White, fontWeight = FontWeight.Bold) },
            text = {
                Text(
                    text = s.t(
                        "确定要永久删除会话「${uiState.selectedSession}」及其关联数据吗？",
                        "Permanently delete session '${uiState.selectedSession}' and its telemetry data?",
                    ),
                    color = GpuColors.Text,
                    fontSize = 13.sp,
                )
            },
            confirmButton = {
                Button(
                    onClick = {
                        deleteConfirm = false
                        vm.deleteCurrentSession()
                    },
                    colors = ButtonDefaults.buttonColors(containerColor = GpuColors.Bad),
                ) { Text(s.t("确认删除", "Delete"), color = Color.White) }
            },
            dismissButton = {
                TextButton(onClick = { deleteConfirm = false }) {
                    Text(s.cancelBtn, color = GpuColors.Dim)
                }
            },
        )
    }

    // 访问口令验证弹窗
    if (uiState.showPinDialog) {
        var pinVal by remember { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = vm::dismissPinDialog,
            containerColor = GpuColors.Card,
            title = { Text(s.t("需要主机访问口令", "Host PIN Required"), color = Color.White, fontWeight = FontWeight.Bold, fontSize = 16.sp) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(
                        s.t("电脑端已启用访问口令保护，验证后方可拉取监控数据。", "Host access PIN protection is enabled. Verify to fetch telemetry."),
                        color = GpuColors.Dim,
                        fontSize = 12.sp,
                    )
                    SettingField(s.t("访问口令", "Access PIN"), pinVal) { pinVal = it }
                    uiState.pinError?.let { err ->
                        Text(err, color = GpuColors.BadLight, fontSize = 11.5.sp)
                    }
                }
            },
            confirmButton = {
                Button(
                    onClick = { vm.submitAuthPin(pinVal) },
                    colors = ButtonDefaults.buttonColors(containerColor = GpuColors.Accent, contentColor = GpuColors.AccentDark),
                ) { Text(s.t("验证解锁", "Unlock"), fontWeight = FontWeight.Bold) }
            },
            dismissButton = {
                TextButton(onClick = vm::dismissPinDialog) { Text(s.cancelBtn, color = GpuColors.Dim) }
            },
        )
    }

    // 远程参数设置弹窗
    if (uiState.showSettings) {
        RemoteSettingsDialog(
            cfg = uiState.settingsConfig,
            gpus = uiState.gpuDevices,
            appLang = uiState.appLang,
            onChangeAppLang = vm::setAppLang,
            onUpdate = vm::updateSettingsConfig,
            onDismiss = vm::closeSettings,
            onSave = vm::saveSettings,
        )
    }

    // 平板双栏模式下的行详情与日志弹窗
    splitDetailRow?.let { r ->
        GpuRowDetailDialog(row = r, onDismiss = { splitDetailRow = null })
    }
    splitLogDetail?.let { entry ->
        LogDetailDialog(
            entry = entry,
            onDismiss = { splitLogDetail = null },
            onToast = vm::emitToast,
        )
    }
}

@Composable
private fun KpiGrid(
    latestGpu: GpuRowDto?,
    data: StateResponse,
    deltaWarn: Double,
    onToggleStartup: () -> Unit,
) {
    val s = LocalStrings.current
    val tempCard: @Composable (Modifier) -> Unit = { mod ->
        KpiCard(modifier = mod, title = s.t("显卡温度 / 功耗", "GPU Temp / Power")) {
            Row(
                verticalAlignment = Alignment.Bottom,
                horizontalArrangement = Arrangement.spacedBy(5.dp),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text(
                    text = latestGpu?.let { "${it.temp}°C" } ?: "—",
                    color = Color.White,
                    fontSize = 15.sp,
                    fontWeight = FontWeight.Bold,
                    fontFamily = FontFamily.Monospace,
                )
                if (!latestGpu?.hotspot.isNullOrEmpty()) {
                    Text(
                        text = s.t("热点${latestGpu!!.hotspot}°", "HS ${latestGpu!!.hotspot}°"),
                        color = GpuColors.Rose,
                        fontSize = 10.5.sp,
                        fontFamily = FontFamily.Monospace,
                        maxLines = 1,
                    )
                }
            }
            Spacer(modifier = Modifier.height(2.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                val deltaVal = latestGpu?.hotspotDeltaVal
                if (deltaVal != null) {
                    Text(
                        text = "Δ${latestGpu.hotspotDeltaText}°C",
                        color = if (deltaVal >= deltaWarn) GpuColors.BadLight else GpuColors.WarnLight,
                        fontSize = 10.5.sp,
                        fontFamily = FontFamily.Monospace,
                    )
                }
                Text(
                    text = latestGpu?.let { "${it.power}W" } ?: "",
                    color = GpuColors.WarnLight,
                    fontSize = 10.5.sp,
                    fontFamily = FontFamily.Monospace,
                )
            }
            data.stats.gpuTemp?.let { st ->
                Spacer(modifier = Modifier.height(1.dp))
                Text(
                    text = s.t("极值 ${st.min.toInt()}~${st.max.toInt()}°C · 均${st.avg.toInt()}°C", "Range ${st.min.toInt()}~${st.max.toInt()}°C · Avg ${st.avg.toInt()}°C"),
                    color = GpuColors.Dim,
                    fontSize = 9.5.sp,
                    fontFamily = FontFamily.Monospace,
                    maxLines = 1,
                )
            }
            Spacer(modifier = Modifier.weight(1f, fill = true))
            Spacer(modifier = Modifier.height(4.dp))
            val utilPct = ((latestGpu?.util?.toFloatOrNull() ?: 0f) / 100f).coerceIn(0f, 1f)
            LinearProgressIndicator(
                progress = { utilPct },
                color = GpuColors.Accent,
                trackColor = GpuColors.InputBg,
                gapSize = 0.dp,
                drawStopIndicator = {},
                modifier = Modifier.fillMaxWidth().height(3.5.dp).clip(RoundedCornerShape(4.dp)),
            )
        }
    }

    val coreCard: @Composable (Modifier) -> Unit = { mod ->
        KpiCard(modifier = mod, title = s.t("核心频率 / 链路", "Core Clock / Link")) {
            Row(
                verticalAlignment = Alignment.Bottom,
                horizontalArrangement = Arrangement.spacedBy(5.dp),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text(
                    text = latestGpu?.let { "${it.core} MHz" } ?: "—",
                    color = Color.White,
                    fontSize = 15.sp,
                    fontWeight = FontWeight.Bold,
                    fontFamily = FontFamily.Monospace,
                    maxLines = 1,
                )
                Text(
                    text = if (latestGpu?.pcieDrop == true) s.t("${latestGpu.pcie} 降速", "${latestGpu.pcie} Drop") else (latestGpu?.pcie ?: ""),
                    color = if (latestGpu?.pcieDrop == true) GpuColors.BadLight else GpuColors.Accent,
                    fontSize = 10.5.sp,
                    fontFamily = FontFamily.Monospace,
                    maxLines = 1,
                )
            }
            Spacer(modifier = Modifier.height(2.dp))
            Text(
                text = buildString {
                    append(s.t("占用 ${latestGpu?.util ?: "—"}% · 显存 ${latestGpu?.vram ?: "—"}MB", "Util ${latestGpu?.util ?: "—"}% · VRAM ${latestGpu?.vram ?: "—"}MB"))
                    if ((latestGpu?.vramPct ?: 0.0) > 0.0) {
                        append(" (${latestGpu!!.vramPct.toInt()}%)")
                    }
                },
                color = GpuColors.Dim,
                fontSize = 10.5.sp,
                fontFamily = FontFamily.Monospace,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(modifier = Modifier.weight(1f, fill = true))
            Spacer(modifier = Modifier.height(4.dp))
            val utilPct2 = ((latestGpu?.util?.toFloatOrNull() ?: 0f) / 100f).coerceIn(0f, 1f)
            LinearProgressIndicator(
                progress = { utilPct2 },
                color = GpuColors.Violet,
                trackColor = GpuColors.InputBg,
                gapSize = 0.dp,
                drawStopIndicator = {},
                modifier = Modifier.fillMaxWidth().height(3.5.dp).clip(RoundedCornerShape(4.dp)),
            )
        }
    }

    val memCard: @Composable (Modifier) -> Unit = { mod ->
        KpiCard(modifier = mod, title = s.t("系统内存提交", "System Memory")) {
            val latestMem = data.memPoints.lastOrNull()
            val memMain = data.memSummary
                .replace("内存", "")
                .trim()
                .split(" ")[0]
                .ifEmpty { "—" }
            val memPctVal = latestMem?.commitPct
            Row(
                verticalAlignment = Alignment.Bottom,
                horizontalArrangement = Arrangement.SpaceBetween,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text(
                    text = memMain,
                    color = Color.White,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Bold,
                    fontFamily = FontFamily.Monospace,
                    maxLines = 1,
                )
                if (memPctVal != null) {
                    Text(
                        text = String.format(Locale.US, "%.0f%%", memPctVal),
                        color = if (memPctVal >= 90.0) GpuColors.BadLight else GpuColors.EmeraldLight,
                        fontSize = 10.5.sp,
                        fontWeight = FontWeight.SemiBold,
                        fontFamily = FontFamily.Monospace,
                    )
                }
            }
            Spacer(modifier = Modifier.weight(1f, fill = true))
            Spacer(modifier = Modifier.height(5.dp))
            val memPct = ((memPctVal?.toFloat() ?: 0f) / 100f).coerceIn(0f, 1f)
            LinearProgressIndicator(
                progress = { memPct },
                color = if (memPct >= 0.9f) GpuColors.Bad else GpuColors.Emerald,
                trackColor = GpuColors.InputBg,
                gapSize = 0.dp,
                drawStopIndicator = {},
                modifier = Modifier.fillMaxWidth().height(3.5.dp).clip(RoundedCornerShape(4.dp)),
            )
        }
    }

    val procCard: @Composable (Modifier) -> Unit = { mod ->
        KpiCard(modifier = mod, title = s.t("目标游戏进程", "Game Process")) {
            val proc = s.trBackend(data.procSummary.ifEmpty { "—" })
            val running = proc != "—" && !proc.contains("未运行")
            Text(
                text = proc,
                color = if (running) GpuColors.EmeraldLight else GpuColors.Dim,
                fontSize = 13.sp,
                fontWeight = FontWeight.Bold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(modifier = Modifier.height(4.dp))
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text(s.t("开机自启", "Startup"), color = GpuColors.Dim, fontSize = 10.5.sp)
                Surface(
                    onClick = onToggleStartup,
                    shape = RoundedCornerShape(5.dp),
                    color = if (data.startupEnabled) GpuColors.EmeraldBg else GpuColors.InputBg,
                    border = BorderStroke(
                        1.dp,
                        if (data.startupEnabled) GpuColors.Emerald.copy(alpha = 0.5f) else GpuColors.Border,
                    ),
                ) {
                    Text(
                        text = if (data.startupEnabled) s.t("已开启", "On") else s.t("未开启", "Off"),
                        color = if (data.startupEnabled) GpuColors.EmeraldLight else GpuColors.Dim,
                        fontSize = 10.sp,
                        fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.padding(horizontal = 6.dp, vertical = 1.5.dp),
                    )
                }
            }
        }
    }

    BoxWithConstraints(modifier = Modifier.fillMaxWidth()) {
        if (maxWidth >= 720.dp) {
            val latestMem = data.memPoints.lastOrNull()
            val memMain = data.memSummary.replace("内存", "").trim().split(" ")[0].ifEmpty { "—" }
            val memPctVal = latestMem?.commitPct
            val memPct = ((memPctVal?.toFloat() ?: 0f) / 100f).coerceIn(0f, 1f)
            val utilPct = ((latestGpu?.util?.toFloatOrNull() ?: 0f) / 100f).coerceIn(0f, 1f)
            val proc = s.trBackend(data.procSummary.ifEmpty { "—" })
            val running = proc != "—" && !proc.contains("未运行")

            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .height(IntrinsicSize.Max),
            ) {
                // 1) 显卡温度 / 功耗（超紧凑 2 行 + 进度条）
                CompactWideKpiCard(
                    modifier = Modifier.weight(1f).fillMaxHeight(),
                    title = s.t("显卡温度 / 功耗", "GPU Temp / Power"),
                    rightTitle = buildString {
                        val dv = latestGpu?.hotspotDeltaVal
                        if (dv != null) append("Δ${latestGpu.hotspotDeltaText}°C  ")
                        if (latestGpu != null) append("${latestGpu.power}W")
                    },
                    rightTitleColor = if ((latestGpu?.hotspotDeltaVal ?: 0.0) >= deltaWarn) GpuColors.Crimson else GpuColors.WarnLight,
                    progress = utilPct,
                    progressColor = GpuColors.Accent,
                ) {
                    Row(
                        verticalAlignment = Alignment.Bottom,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Text(
                            text = latestGpu?.let { "${it.temp}°C" } ?: "—",
                            color = Color.White,
                            fontSize = 15.sp,
                            fontWeight = FontWeight.Bold,
                            fontFamily = FontFamily.Monospace,
                        )
                        if (!latestGpu?.hotspot.isNullOrEmpty()) {
                            Text(
                                text = s.t("热点 ${latestGpu!!.hotspot}°", "HS ${latestGpu!!.hotspot}°"),
                                color = GpuColors.Rose,
                                fontSize = 10.5.sp,
                                fontFamily = FontFamily.Monospace,
                                maxLines = 1,
                            )
                        }
                    }
                }

                // 2) 核心频率 / 链路（超紧凑 2 行 + 进度条）
                CompactWideKpiCard(
                    modifier = Modifier.weight(1f).fillMaxHeight(),
                    title = s.t("核心频率 / 链路", "Core Clock / Link"),
                    rightTitle = latestGpu?.pcie ?: "",
                    rightTitleColor = GpuColors.Accent,
                    progress = utilPct,
                    progressColor = GpuColors.Violet,
                ) {
                    Row(
                        verticalAlignment = Alignment.Bottom,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Text(
                            text = latestGpu?.let { "${it.core} MHz" } ?: "—",
                            color = Color.White,
                            fontSize = 14.5.sp,
                            fontWeight = FontWeight.Bold,
                            fontFamily = FontFamily.Monospace,
                            maxLines = 1,
                        )
                        Text(
                            text = "${latestGpu?.util ?: "—"}% · ${latestGpu?.vram ?: "—"}MB",
                            color = GpuColors.Dim,
                            fontSize = 10.5.sp,
                            fontFamily = FontFamily.Monospace,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }

                // 3) 系统内存提交（超紧凑 2 行 + 进度条）
                CompactWideKpiCard(
                    modifier = Modifier.weight(1f).fillMaxHeight(),
                    title = s.t("系统内存提交", "System Memory"),
                    rightTitle = memPctVal?.let { String.format(Locale.US, "%.0f%%", it) } ?: "",
                    rightTitleColor = if ((memPctVal ?: 0.0) >= 90.0) GpuColors.BadLight else GpuColors.EmeraldLight,
                    progress = memPct,
                    progressColor = if (memPct >= 0.9f) GpuColors.Bad else GpuColors.Emerald,
                ) {
                    Text(
                        text = memMain,
                        color = Color.White,
                        fontSize = 14.sp,
                        fontWeight = FontWeight.Bold,
                        fontFamily = FontFamily.Monospace,
                        maxLines = 1,
                    )
                }

                // 4) 目标游戏进程与开机自启（超紧凑 2 行）
                Surface(
                    shape = RoundedCornerShape(10.dp),
                    color = GpuColors.Card,
                    border = BorderStroke(1.dp, GpuColors.Border),
                    modifier = Modifier.weight(1f).fillMaxHeight(),
                ) {
                    Column(
                        verticalArrangement = Arrangement.SpaceBetween,
                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.SpaceBetween,
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Text(text = s.t("目标游戏进程", "Game Process"), color = GpuColors.Dim, fontSize = 10.5.sp)
                            Surface(
                                onClick = onToggleStartup,
                                shape = RoundedCornerShape(5.dp),
                                color = if (data.startupEnabled) GpuColors.EmeraldBg else GpuColors.InputBg,
                                border = BorderStroke(
                                    1.dp,
                                    if (data.startupEnabled) GpuColors.Emerald.copy(alpha = 0.5f) else GpuColors.Border,
                                ),
                            ) {
                                Text(
                                    text = if (data.startupEnabled) s.t("自启:开", "Boot:On") else s.t("自启:关", "Boot:Off"),
                                    color = if (data.startupEnabled) GpuColors.EmeraldLight else GpuColors.Dim,
                                    fontSize = 9.5.sp,
                                    fontWeight = FontWeight.SemiBold,
                                    modifier = Modifier.padding(horizontal = 5.dp, vertical = 1.dp),
                                )
                            }
                        }
                        Spacer(modifier = Modifier.height(3.dp))
                        Text(
                            text = proc,
                            color = if (running) GpuColors.EmeraldLight else GpuColors.Dim,
                            fontSize = 13.5.sp,
                            fontWeight = FontWeight.Bold,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(5.dp)) {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(IntrinsicSize.Max),
                ) {
                    tempCard(Modifier.weight(1f).fillMaxHeight())
                    coreCard(Modifier.weight(1f).fillMaxHeight())
                }
                Row(
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(IntrinsicSize.Max),
                ) {
                    memCard(Modifier.weight(1f).fillMaxHeight())
                    procCard(Modifier.weight(1f).fillMaxHeight())
                }
            }
        }
    }
}

@Composable
private fun CpuRingGauge(
    progress: Float,
    color: Color,
    size: Dp,
    strokeWidth: Dp,
    centerText: String,
    fontSize: androidx.compose.ui.unit.TextUnit,
) {
    Box(
        contentAlignment = Alignment.Center,
        modifier = Modifier.size(size),
    ) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            val sw = strokeWidth.toPx()
            drawArc(
                color = Color.White.copy(alpha = 0.09f),
                startAngle = 0f,
                sweepAngle = 360f,
                useCenter = false,
                style = Stroke(width = sw, cap = StrokeCap.Round),
            )
            val sweep = 360f * progress.coerceIn(0f, 1f)
            if (sweep > 0.5f) {
                drawArc(
                    color = color,
                    startAngle = -90f,
                    sweepAngle = sweep,
                    useCenter = false,
                    style = Stroke(width = sw, cap = StrokeCap.Round),
                )
            }
        }
        Text(
            text = centerText,
            color = Color.White,
            fontSize = fontSize,
            fontWeight = FontWeight.Bold,
            fontFamily = FontFamily.Monospace,
        )
    }
}

@Composable
internal fun CpuMonitorTab(
    cpu: CpuSnapshotDto?,
    cpuPoints: List<LiveCpuPointDto> = emptyList(),
    liveRunning: Boolean = false,
    onCopySummary: () -> Unit = {},
    onSaveSession: () -> Unit = {},
) {
    val s = LocalStrings.current
    if (cpu == null) {
        EmptyPlaceholder(s.t("请点击「实时监测」开始采集 CPU 数据", "Tap 'Live Mon' to start CPU monitoring"))
        return
    }

    val pColor = Color(0xFF60A5FA)
    val eColor = GpuColors.EmeraldLight
    val accent = Color(0xFF38BDF8)

    BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
        val wideSummary = maxWidth >= 600.dp
        val coreCols = when {
            maxWidth >= 960.dp -> 8
            maxWidth >= 700.dp -> 6
            maxWidth >= 480.dp -> 4
            else -> 3
        }

        var coresExpanded by remember { mutableStateOf(false) }

        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            item {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(
                        text = if (liveRunning) s.t("实时监测中（纯内存缓冲）", "Live Monitoring (In-Memory)") else s.t("已停止监测（保留本次快照）", "Stopped (Snapshot Retained)"),
                        color = GpuColors.Dim,
                        fontSize = 11.5.sp,
                        modifier = Modifier.weight(1f),
                    )
                    SmallActionChip(
                        icon = Icons.Rounded.ContentCopy,
                        label = s.t("复制摘要", "Copy Summary"),
                        fg = GpuColors.Accent,
                        bg = GpuColors.AccentBg,
                        border = GpuColors.AccentBorder,
                        onClick = onCopySummary,
                    )
                    if (!liveRunning) {
                        SmallActionChip(
                            icon = Icons.Rounded.Save,
                            label = s.t("补存会话", "Save Session"),
                            fg = GpuColors.EmeraldLight,
                            bg = GpuColors.EmeraldBg,
                            border = GpuColors.EmeraldBorder,
                            onClick = onSaveSession,
                        )
                    }
                }
            }
            item {
                val totalCard: @Composable (Modifier) -> Unit = { mod ->
                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        color = GpuColors.Card,
                        border = BorderStroke(1.dp, GpuColors.Border),
                        modifier = mod,
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(12.dp),
                            modifier = Modifier.padding(12.dp),
                        ) {
                            CpuRingGauge(
                                progress = (cpu.totalUsage / 100.0).toFloat(),
                                color = accent,
                                size = 64.dp,
                                strokeWidth = 6.dp,
                                centerText = "${cpu.totalUsage.toInt()}%",
                                fontSize = 14.sp,
                            )
                            Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                                Text(
                                    text = cpu.brand,
                                    color = Color.White,
                                    fontSize = 12.5.sp,
                                    fontWeight = FontWeight.Bold,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                val tStr = cpu.tempC?.let { String.format(Locale.US, "%.1f°C", it) } ?: "—"
                                val tCol = when {
                                    (cpu.tempC ?: 0.0) >= 85.0 -> GpuColors.BadLight
                                    (cpu.tempC ?: 0.0) >= 75.0 -> GpuColors.WarnLight
                                    else -> accent
                                }
                                Text(
                                    text = s.t("温度 $tStr · ${cpu.physicalCores}核${cpu.logicalCores}框", "Temp $tStr · ${cpu.physicalCores}C/${cpu.logicalCores}T"),
                                    color = tCol,
                                    fontSize = 11.5.sp,
                                    fontFamily = FontFamily.Monospace,
                                )
                                Text(
                                    text = s.t("均频 ${cpu.avgFreqMhz} MHz · 峰值 ${cpu.maxFreqMhz} MHz", "Avg ${cpu.avgFreqMhz} MHz · Max ${cpu.maxFreqMhz} MHz"),
                                    color = GpuColors.Dim,
                                    fontSize = 11.sp,
                                    fontFamily = FontFamily.Monospace,
                                )
                            }
                        }
                    }
                }

                val pCard: @Composable (Modifier) -> Unit = { mod ->
                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        color = GpuColors.Card,
                        border = BorderStroke(1.dp, pColor.copy(alpha = 0.35f)),
                        modifier = mod,
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                            modifier = Modifier.padding(12.dp),
                        ) {
                            CpuRingGauge(
                                progress = (cpu.pUsage / 100.0).toFloat(),
                                color = pColor,
                                size = 54.dp,
                                strokeWidth = 5.dp,
                                centerText = "${cpu.pUsage.toInt()}%",
                                fontSize = 12.sp,
                            )
                            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                                Text(
                                    text = s.t("P 性能核 ×${cpu.pCount}", "P-Core ×${cpu.pCount}"),
                                    color = pColor,
                                    fontSize = 12.sp,
                                    fontWeight = FontWeight.Bold,
                                )
                                Text(
                                    text = "${cpu.pFreqMhz} MHz",
                                    color = GpuColors.Text,
                                    fontSize = 11.5.sp,
                                    fontFamily = FontFamily.Monospace,
                                )
                            }
                        }
                    }
                }

                val eCard: @Composable (Modifier) -> Unit = { mod ->
                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        color = GpuColors.Card,
                        border = BorderStroke(1.dp, eColor.copy(alpha = 0.35f)),
                        modifier = mod,
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                            modifier = Modifier.padding(12.dp),
                        ) {
                            CpuRingGauge(
                                progress = (cpu.eUsage / 100.0).toFloat(),
                                color = eColor,
                                size = 54.dp,
                                strokeWidth = 5.dp,
                                centerText = "${cpu.eUsage.toInt()}%",
                                fontSize = 12.sp,
                            )
                            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                                Text(
                                    text = s.t("E 能效核 ×${cpu.eCount}", "E-Core ×${cpu.eCount}"),
                                    color = eColor,
                                    fontSize = 12.sp,
                                    fontWeight = FontWeight.Bold,
                                )
                                Text(
                                    text = "${cpu.eFreqMhz} MHz",
                                    color = GpuColors.Text,
                                    fontSize = 11.5.sp,
                                    fontFamily = FontFamily.Monospace,
                                )
                            }
                        }
                    }
                }

                if (wideSummary) {
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier.fillMaxWidth().height(IntrinsicSize.Max),
                    ) {
                        totalCard(Modifier.weight(1.4f).fillMaxHeight())
                        if (cpu.hasHybrid) {
                            pCard(Modifier.weight(1f).fillMaxHeight())
                            eCard(Modifier.weight(1f).fillMaxHeight())
                        }
                    }
                } else {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        totalCard(Modifier.fillMaxWidth())
                        if (cpu.hasHybrid) {
                            Row(
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                                modifier = Modifier.fillMaxWidth().height(IntrinsicSize.Max),
                            ) {
                                pCard(Modifier.weight(1f).fillMaxHeight())
                                eCard(Modifier.weight(1f).fillMaxHeight())
                            }
                        }
                    }
                }
            }

            if (cpuPoints.isNotEmpty()) {
                item {
                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        color = GpuColors.Card,
                        border = BorderStroke(1.dp, GpuColors.Border),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Column(
                            verticalArrangement = Arrangement.spacedBy(6.dp),
                            modifier = Modifier.padding(12.dp),
                        ) {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.SpaceBetween,
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Text(
                                    text = s.t("CPU 60 秒实时趋势与异常点", "CPU 60s Live Trend & Alerts"),
                                    color = Color.White,
                                    fontSize = 12.5.sp,
                                    fontWeight = FontWeight.Bold,
                                )
                                Text(
                                    text = "${cpuPoints.first().ts} ~ ${cpuPoints.last().ts}",
                                    color = GpuColors.Dim,
                                    fontSize = 10.5.sp,
                                    fontFamily = FontFamily.Monospace,
                                )
                            }
                            androidx.compose.foundation.Canvas(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .height(105.dp),
                            ) {
                                val n = cpuPoints.size
                                val w = size.width
                                val h = size.height
                                fun xAt(idx: Int) = if (n <= 1) w / 2f else (idx.toFloat() / (n - 1)) * w
                                fun yAt(v: Double) = h * (1f - (v.coerceIn(0.0, 100.0) / 100.0).toFloat())

                                val usagePath = androidx.compose.ui.graphics.Path()
                                val tempPath = androidx.compose.ui.graphics.Path()
                                var tempStarted = false
                                cpuPoints.forEachIndexed { i, p ->
                                    val x = xAt(i)
                                    val y = yAt(p.totalUsage)
                                    if (i == 0) usagePath.moveTo(x, y) else usagePath.lineTo(x, y)
                                    p.tempC?.let { t ->
                                        val ty = yAt(t)
                                        if (!tempStarted) {
                                            tempPath.moveTo(x, ty)
                                            tempStarted = true
                                        } else {
                                            tempPath.lineTo(x, ty)
                                        }
                                    }
                                }
                                if (tempStarted) {
                                    drawPath(
                                        path = tempPath,
                                        color = GpuColors.WarnLight,
                                        style = Stroke(width = 1.8.dp.toPx()),
                                    )
                                }
                                drawPath(
                                    path = usagePath,
                                    color = accent,
                                    style = Stroke(width = 2.2.dp.toPx()),
                                )
                                cpuPoints.forEachIndexed { i, p ->
                                    if (p.alert) {
                                        drawCircle(
                                            color = GpuColors.BadLight,
                                            radius = 4.dp.toPx(),
                                            center = androidx.compose.ui.geometry.Offset(xAt(i), yAt(p.totalUsage)),
                                        )
                                    }
                                }
                            }
                            Text(
                                text = s.t("蓝线：CPU 占用 % · 橙线：CPU 温度 °C · 红点：异常/降频时刻", "Blue: CPU % · Orange: Temp °C · Red: Alert/Throttle"),
                                color = GpuColors.Dim,
                                fontSize = 10.sp,
                            )
                        }
                    }
                }
            }

            if (cpu.topCpuProcs.isNotEmpty() || cpu.topMemProcs.isNotEmpty()) {
                item {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (cpu.topCpuProcs.isNotEmpty()) {
                            Surface(
                                shape = RoundedCornerShape(12.dp),
                                color = GpuColors.Card,
                                border = BorderStroke(1.dp, GpuColors.Border),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Column(
                                    verticalArrangement = Arrangement.spacedBy(4.dp),
                                    modifier = Modifier.padding(12.dp),
                                ) {
                                    Text(s.t("Top 5 CPU 占用进程", "Top 5 CPU Processes"), color = accent, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                                    cpu.topCpuProcs.forEach { p ->
                                        Row(modifier = Modifier.fillMaxWidth()) {
                                            Text(
                                                text = "${p.name} (${p.pid})",
                                                color = GpuColors.Text,
                                                fontSize = 11.sp,
                                                fontFamily = FontFamily.Monospace,
                                                maxLines = 1,
                                                overflow = TextOverflow.Ellipsis,
                                                modifier = Modifier.weight(1f),
                                            )
                                            Text(
                                                text = "${String.format(Locale.US, "%.1f", p.cpuPct)}% · ${p.memMb} MB",
                                                color = accent,
                                                fontSize = 11.sp,
                                                fontFamily = FontFamily.Monospace,
                                            )
                                        }
                                    }
                                }
                            }
                        }
                        if (cpu.topMemProcs.isNotEmpty()) {
                            Surface(
                                shape = RoundedCornerShape(12.dp),
                                color = GpuColors.Card,
                                border = BorderStroke(1.dp, GpuColors.Border),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Column(
                                    verticalArrangement = Arrangement.spacedBy(4.dp),
                                    modifier = Modifier.padding(12.dp),
                                ) {
                                    Text(s.t("Top 5 内存占用进程", "Top 5 Memory Processes"), color = eColor, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                                    cpu.topMemProcs.forEach { p ->
                                        Row(modifier = Modifier.fillMaxWidth()) {
                                            Text(
                                                text = "${p.name} (${p.pid})",
                                                color = GpuColors.Text,
                                                fontSize = 11.sp,
                                                fontFamily = FontFamily.Monospace,
                                                maxLines = 1,
                                                overflow = TextOverflow.Ellipsis,
                                                modifier = Modifier.weight(1f),
                                            )
                                            Text(
                                                text = "${p.memMb} MB · ${String.format(Locale.US, "%.1f", p.cpuPct)}%",
                                                color = eColor,
                                                fontSize = 11.sp,
                                                fontFamily = FontFamily.Monospace,
                                            )
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            item {
                Surface(
                    onClick = { coresExpanded = !coresExpanded },
                    shape = RoundedCornerShape(9.dp),
                    color = GpuColors.SurfaceBtn,
                    border = BorderStroke(1.dp, GpuColors.Border),
                ) {
                    Text(
                        text = if (coresExpanded) s.t("▼ 收起逐核环形负载（${cpu.cores.size} 核）", "▼ Hide Per-Core Ring Load (${cpu.cores.size}T)")
                        else s.t("▶ 展开逐核环形负载（${cpu.cores.size} 核）", "▶ Show Per-Core Ring Load (${cpu.cores.size}T)"),
                        color = GpuColors.Text,
                        fontSize = 11.5.sp,
                        fontWeight = FontWeight.Medium,
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
                    )
                }
            }

            if (coresExpanded) {
                items(cpu.cores.chunked(coreCols)) { rowCores ->
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        rowCores.forEach { c ->
                            val kCol = when (c.kind) {
                                "P" -> pColor
                                "E" -> eColor
                                else -> accent
                            }
                            Surface(
                                shape = RoundedCornerShape(10.dp),
                                color = GpuColors.Card,
                                border = BorderStroke(1.dp, GpuColors.Border),
                                modifier = Modifier.weight(1f),
                            ) {
                                Column(
                                    horizontalAlignment = Alignment.CenterHorizontally,
                                    modifier = Modifier.padding(horizontal = 6.dp, vertical = 8.dp),
                                ) {
                                    CpuRingGauge(
                                        progress = (c.usage / 100.0).toFloat(),
                                        color = kCol,
                                        size = 44.dp,
                                        strokeWidth = 4.dp,
                                        centerText = "${c.usage.toInt()}%",
                                        fontSize = 10.sp,
                                    )
                                    Spacer(modifier = Modifier.height(5.dp))
                                    Text(
                                        text = "C${c.id} · ${c.kind}",
                                        color = kCol,
                                        fontSize = 10.5.sp,
                                        fontWeight = FontWeight.Bold,
                                        fontFamily = FontFamily.Monospace,
                                    )
                                    Text(
                                        text = "${c.freqMhz}MHz",
                                        color = GpuColors.Dim,
                                        fontSize = 9.5.sp,
                                        fontFamily = FontFamily.Monospace,
                                    )
                                }
                            }
                        }
                        repeat(coreCols - rowCores.size) {
                            Spacer(modifier = Modifier.weight(1f))
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun CompactWideKpiCard(
    modifier: Modifier = Modifier,
    title: String,
    rightTitle: String,
    rightTitleColor: Color,
    progress: Float,
    progressColor: Color,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(
        shape = RoundedCornerShape(10.dp),
        color = GpuColors.Card,
        border = BorderStroke(1.dp, GpuColors.Border),
        modifier = modifier,
    ) {
        Column(modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text(text = title, color = GpuColors.Dim, fontSize = 10.5.sp, maxLines = 1)
                if (rightTitle.isNotEmpty()) {
                    Text(
                        text = rightTitle,
                        color = rightTitleColor,
                        fontSize = 10.5.sp,
                        fontWeight = FontWeight.SemiBold,
                        fontFamily = FontFamily.Monospace,
                        maxLines = 1,
                    )
                }
            }
            Spacer(modifier = Modifier.height(3.dp))
            content()
            Spacer(modifier = Modifier.weight(1f, fill = true))
            Spacer(modifier = Modifier.height(4.dp))
            LinearProgressIndicator(
                progress = { progress },
                color = progressColor,
                trackColor = GpuColors.InputBg,
                gapSize = 0.dp,
                drawStopIndicator = {},
                modifier = Modifier.fillMaxWidth().height(3.dp).clip(RoundedCornerShape(3.dp)),
            )
        }
    }
}

@Composable
private fun KpiCard(
    modifier: Modifier = Modifier,
    title: String,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(
        shape = RoundedCornerShape(10.dp),
        color = GpuColors.Card,
        border = BorderStroke(1.dp, GpuColors.Border),
        modifier = modifier,
    ) {
        Column(modifier = Modifier.padding(horizontal = 9.dp, vertical = 7.dp)) {
            Text(text = title, color = GpuColors.Dim, fontSize = 10.5.sp)
            Spacer(modifier = Modifier.height(3.dp))
            content()
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun SessionToolbar(
    uiState: MainUiState,
    wrapMode: Boolean = false,
    onSelectSession: (String) -> Unit,
    onTogglePause: () -> Unit,
    onToggleOnlyBad: () -> Unit,
    onLoadFull: () -> Unit,
    onExport: () -> Unit,
    onRequestDelete: () -> Unit,
) {
    val s = LocalStrings.current
    var expanded by remember { mutableStateOf(false) }
    val sessions = uiState.monitorState.sessions

    val toolbarItems: @Composable () -> Unit = {
        Box {
            Surface(
                onClick = { expanded = true },
                shape = RoundedCornerShape(7.dp),
                color = GpuColors.Card,
                border = BorderStroke(1.dp, GpuColors.Border),
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 5.dp),
                ) {
                    Text(
                        text = when (uiState.selectedSession) {
                            "" -> s.t("选择会话", "Session")
                            "__live__" -> s.t("实时监测(仅内存)", "Live (Memory)")
                            else -> uiState.selectedSession
                        },
                        color = GpuColors.Text,
                        fontSize = 11.sp,
                        fontFamily = FontFamily.Monospace,
                    )
                    Spacer(modifier = Modifier.width(2.dp))
                    Icon(
                        imageVector = Icons.Rounded.KeyboardArrowDown,
                        contentDescription = null,
                        tint = GpuColors.Dim,
                        modifier = Modifier.size(14.dp),
                    )
                }
            }
            DropdownMenu(
                expanded = expanded,
                onDismissRequest = { expanded = false },
                containerColor = GpuColors.Card,
            ) {
                sessions.forEach { sItem ->
                    DropdownMenuItem(
                        text = {
                            Text(
                                text = "${if (sItem.active) s.t("[运行] ", "[Active] ") else ""}${if (sItem.name == "__live__") s.t("实时监测(仅内存)", "Live (Memory)") else sItem.name} (${sItem.sizeLabel})",
                                color = if (sItem.name == uiState.selectedSession) GpuColors.Accent else GpuColors.Text,
                                fontSize = 12.5.sp,
                                fontFamily = FontFamily.Monospace,
                            )
                        },
                        onClick = {
                            expanded = false
                            onSelectSession(sItem.name)
                        },
                    )
                }
            }
        }

        SmallActionChip(
            icon = if (uiState.paused) Icons.Rounded.PlayArrow else Icons.Rounded.Pause,
            label = if (uiState.paused) s.resumeUi else s.pauseUi,
            fg = if (uiState.paused) GpuColors.WarnLight else GpuColors.Text,
            bg = if (uiState.paused) GpuColors.WarnBg else GpuColors.Card,
            border = if (uiState.paused) GpuColors.WarnBorder else GpuColors.Border,
            onClick = onTogglePause,
        )

        SmallActionChip(
            icon = Icons.Rounded.WarningAmber,
            label = s.t("异常", "Alerts"),
            fg = if (uiState.onlyBad) GpuColors.BadLight else GpuColors.Text,
            bg = if (uiState.onlyBad) GpuColors.BadBg else GpuColors.Card,
            border = if (uiState.onlyBad) GpuColors.BadBorder else GpuColors.Border,
            onClick = onToggleOnlyBad,
        )

        if (!uiState.fullHistory) {
            SmallActionChip(
                icon = Icons.Rounded.Refresh,
                label = s.t("全部", "All"),
                fg = GpuColors.Text,
                bg = GpuColors.Card,
                border = GpuColors.Border,
                onClick = onLoadFull,
            )
        }

        if (uiState.selectedSession != "__live__") {
            SmallActionChip(
                icon = Icons.Rounded.Download,
                label = null,
                fg = GpuColors.Accent,
                bg = GpuColors.AccentBg,
                border = GpuColors.AccentBorder,
                onClick = onExport,
            )
        }

        if (uiState.selectedSession.isNotEmpty() &&
            uiState.selectedSession != "__live__" &&
            sessions.find { it.name == uiState.selectedSession }?.active != true
        ) {
            SmallActionChip(
                icon = Icons.Rounded.DeleteOutline,
                label = null,
                fg = GpuColors.BadLight,
                bg = GpuColors.BadBg,
                border = GpuColors.BadBorder,
                onClick = onRequestDelete,
            )
        }
    }

    if (wrapMode) {
        Surface(
            shape = RoundedCornerShape(12.dp),
            color = GpuColors.Card,
            border = BorderStroke(1.dp, GpuColors.Border),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Column(
                modifier = Modifier.padding(9.dp),
                verticalArrangement = Arrangement.spacedBy(7.dp),
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(
                        text = s.t("会话管理", "Sessions"),
                        color = GpuColors.Dim,
                        fontSize = 11.sp,
                        fontWeight = FontWeight.Medium,
                        modifier = Modifier.weight(1f),
                    )
                    if (uiState.selectedSession != "__live__") {
                        SmallActionChip(
                            icon = Icons.Rounded.Download,
                            label = null,
                            fg = GpuColors.Accent,
                            bg = GpuColors.AccentBg,
                            border = GpuColors.AccentBorder,
                            onClick = onExport,
                        )
                    }
                    if (uiState.selectedSession.isNotEmpty() &&
                        uiState.selectedSession != "__live__" &&
                        sessions.find { it.name == uiState.selectedSession }?.active != true
                    ) {
                        SmallActionChip(
                            icon = Icons.Rounded.DeleteOutline,
                            label = null,
                            fg = GpuColors.BadLight,
                            bg = GpuColors.BadBg,
                            border = GpuColors.BadBorder,
                            onClick = onRequestDelete,
                        )
                    }
                }

                Box(modifier = Modifier.fillMaxWidth()) {
                    Surface(
                        onClick = { expanded = true },
                        shape = RoundedCornerShape(8.dp),
                        color = GpuColors.InputBg,
                        border = BorderStroke(1.dp, GpuColors.Border),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.SpaceBetween,
                            modifier = Modifier.padding(horizontal = 9.dp, vertical = 6.dp),
                        ) {
                            Text(
                                text = when (uiState.selectedSession) {
                                    "" -> s.t("选择会话", "Session")
                                    "__live__" -> s.t("实时监测(仅内存)", "Live (Memory)")
                                    else -> uiState.selectedSession
                                },
                                color = Color.White,
                                fontSize = 11.5.sp,
                                fontFamily = FontFamily.Monospace,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            Icon(
                                imageVector = Icons.Rounded.KeyboardArrowDown,
                                contentDescription = null,
                                tint = GpuColors.Dim,
                                modifier = Modifier.size(15.dp),
                            )
                        }
                    }
                    DropdownMenu(
                        expanded = expanded,
                        onDismissRequest = { expanded = false },
                        containerColor = GpuColors.Card,
                    ) {
                        sessions.forEach { sItem ->
                            DropdownMenuItem(
                                text = {
                                    Text(
                                        text = "${if (sItem.active) s.t("[运行] ", "[Active] ") else ""}${if (sItem.name == "__live__") s.t("实时监测(仅内存)", "Live (Memory)") else sItem.name} (${sItem.sizeLabel})",
                                        color = if (sItem.name == uiState.selectedSession) GpuColors.Accent else GpuColors.Text,
                                        fontSize = 12.5.sp,
                                        fontFamily = FontFamily.Monospace,
                                    )
                                },
                                onClick = {
                                    expanded = false
                                    onSelectSession(sItem.name)
                                },
                            )
                        }
                    }
                }

                Row(
                    horizontalArrangement = Arrangement.spacedBy(5.dp),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    SmallActionChip(
                        icon = if (uiState.paused) Icons.Rounded.PlayArrow else Icons.Rounded.Pause,
                        label = if (uiState.paused) s.resumeUi else s.pauseUi,
                        fg = if (uiState.paused) GpuColors.WarnLight else GpuColors.Text,
                        bg = if (uiState.paused) GpuColors.WarnBg else GpuColors.InputBg,
                        border = if (uiState.paused) GpuColors.WarnBorder else GpuColors.Border,
                        modifier = Modifier.weight(1f),
                        onClick = onTogglePause,
                    )
                    SmallActionChip(
                        icon = Icons.Rounded.WarningAmber,
                        label = s.t("异常", "Alerts"),
                        fg = if (uiState.onlyBad) GpuColors.BadLight else GpuColors.Text,
                        bg = if (uiState.onlyBad) GpuColors.BadBg else GpuColors.InputBg,
                        border = if (uiState.onlyBad) GpuColors.BadBorder else GpuColors.Border,
                        modifier = Modifier.weight(1f),
                        onClick = onToggleOnlyBad,
                    )
                    if (!uiState.fullHistory) {
                        SmallActionChip(
                            icon = Icons.Rounded.Refresh,
                            label = s.t("全部", "All"),
                            fg = GpuColors.Text,
                            bg = GpuColors.InputBg,
                            border = GpuColors.Border,
                            modifier = Modifier.weight(1f),
                            onClick = onLoadFull,
                        )
                    }
                }
            }
        }
    } else {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(5.dp),
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState()),
        ) {
            toolbarItems()
        }
    }
}

@Composable
private fun DetailEntryCard(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    accent: Color,
    title: String,
    subtitle: String,
    badge: String,
    selected: Boolean = false,
    compact: Boolean = false,
    onClick: () -> Unit,
) {
    Surface(
        onClick = onClick,
        shape = RoundedCornerShape(9.dp),
        color = when {
            selected -> GpuColors.AccentBg
            compact -> GpuColors.InputBg.copy(alpha = 0.7f)
            else -> GpuColors.Card
        },
        border = BorderStroke(
            1.dp,
            if (selected) GpuColors.Accent.copy(alpha = 0.7f)
            else if (compact) GpuColors.Border.copy(alpha = 0.6f)
            else GpuColors.Border,
        ),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(if (compact) 7.dp else 9.dp),
            modifier = Modifier.padding(horizontal = if (compact) 8.dp else 10.dp, vertical = if (compact) 5.dp else 7.dp),
        ) {
            Surface(
                shape = RoundedCornerShape(7.dp),
                color = accent.copy(alpha = 0.14f),
                border = BorderStroke(1.dp, accent.copy(alpha = 0.35f)),
                modifier = Modifier.size(if (compact) 24.dp else 28.dp),
            ) {
                Box(contentAlignment = Alignment.Center) {
                    Icon(
                        imageVector = icon,
                        contentDescription = null,
                        tint = accent,
                        modifier = Modifier.size(if (compact) 14.dp else 16.dp),
                    )
                }
            }
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = title,
                    color = Color.White,
                    fontSize = if (compact) 12.5.sp else 13.sp,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (!compact) {
                    Text(
                        text = subtitle,
                        color = GpuColors.Dim,
                        fontSize = 10.5.sp,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            Surface(
                shape = RoundedCornerShape(99.dp),
                color = GpuColors.InputBg,
                border = BorderStroke(1.dp, GpuColors.Border),
            ) {
                Text(
                    text = badge,
                    color = accent,
                    fontSize = 10.sp,
                    fontFamily = FontFamily.Monospace,
                    modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                )
            }
            if (!compact) {
                Icon(
                    imageVector = Icons.AutoMirrored.Rounded.KeyboardArrowRight,
                    contentDescription = null,
                    tint = GpuColors.Dim,
                    modifier = Modifier.size(16.dp),
                )
            }
        }
    }
}

@Composable
internal fun MonitorTabBar(
    activeTab: MonitorTab,
    data: StateResponse,
    onSelectTab: (MonitorTab) -> Unit,
) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(5.dp),
        modifier = Modifier
            .fillMaxWidth()
            .height(IntrinsicSize.Max),
    ) {
        val tabs = remember(data.cpu != null) {
            MonitorTab.entries.filter { it != MonitorTab.Cpu || data.cpu != null }
        }
        tabs.forEach { tab ->
            val selected = activeTab == tab
            val count = when (tab) {
                MonitorTab.Gpu -> data.gpuRows.size
                MonitorTab.Cpu -> data.cpu?.logicalCores ?: 0
                MonitorTab.Trend -> 0
                MonitorTab.Events -> data.events.size
                MonitorTab.Wer -> data.wer.size
                MonitorTab.Alerts -> data.alerts.size
            }
            Surface(
                onClick = { onSelectTab(tab) },
                shape = RoundedCornerShape(9.dp),
                color = if (selected) GpuColors.AccentBg else GpuColors.Card,
                border = BorderStroke(
                    1.dp,
                    if (selected) GpuColors.Accent.copy(alpha = 0.65f) else GpuColors.Border,
                ),
                modifier = Modifier.weight(1f).fillMaxHeight(),
            ) {
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                    modifier = Modifier.padding(horizontal = 4.dp, vertical = 6.dp),
                ) {
                    Text(
                        text = LocalStrings.current.tabLabel(tab),
                        color = if (selected) Color.White else GpuColors.Dim,
                        fontSize = 11.5.sp,
                        fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
                        maxLines = 1,
                    )
                    if (count > 0) {
                        Text(
                            text = count.toString(),
                            color = if (selected) GpuColors.Accent else GpuColors.Dim,
                            fontSize = 10.sp,
                            fontFamily = FontFamily.Monospace,
                            maxLines = 1,
                        )
                    }
                }
            }
        }
    }
}

@Composable
internal fun LogDetailDialog(
    entry: LogEntryDto,
    onDismiss: () -> Unit,
    onToast: (String) -> Unit,
) {
    val clipboard = LocalClipboardManager.current
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = GpuColors.Card,
        title = {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(entry.level, color = sevTextColor(entry.sev), fontWeight = FontWeight.Bold, fontSize = 15.sp)
                Text(entry.time, color = GpuColors.Dim, fontFamily = FontFamily.Monospace, fontSize = 12.sp)
            }
        },
        text = {
            val s = LocalStrings.current
            Column(
                modifier = Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(s.t("来源：${entry.source}", "Source: ${entry.source}"), color = GpuColors.Accent, fontSize = 12.sp)
                Surface(
                    color = GpuColors.InputBg,
                    shape = RoundedCornerShape(10.dp),
                    border = BorderStroke(1.dp, GpuColors.Border),
                ) {
                    Text(
                        text = entry.raw.ifEmpty { entry.message },
                        color = GpuColors.Text,
                        fontSize = 12.sp,
                        fontFamily = FontFamily.Monospace,
                        modifier = Modifier.padding(10.dp),
                    )
                }
            }
        },
        confirmButton = {
            val s = LocalStrings.current
            Button(
                onClick = {
                    clipboard.setText(AnnotatedString(entry.raw.ifEmpty { entry.message }))
                    onToast(s.t("已复制完整日志", "Full log entry copied"))
                    onDismiss()
                },
                colors = ButtonDefaults.buttonColors(containerColor = GpuColors.Accent, contentColor = GpuColors.AccentDark),
            ) { Text(s.t("复制内容", "Copy"), fontWeight = FontWeight.Bold) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(LocalStrings.current.closeBtn, color = GpuColors.Dim) }
        },
    )
}

@Composable
internal fun TrendTab(
    gpuRows: List<GpuRowDto>,
    memPoints: List<MemPointDto>,
    windowSize: Int,
    onChangeWindow: (Int) -> Unit,
) {
    val s = LocalStrings.current
    val slicedGpu = remember(gpuRows, windowSize) { gpuRows.takeLast(windowSize) }
    val slicedMem = remember(memPoints, windowSize) { memPoints.takeLast(windowSize.coerceAtMost(30)) }

    val specs = remember(slicedGpu, slicedMem, s.isZh) {
        fun mapGpu(pick: (GpuRowDto) -> Double?): List<TrendPoint> =
            slicedGpu.mapNotNull { r -> pick(r)?.let { TrendPoint(r.ts, it) } }

        listOf(
            TrendMetricSpec(s.t("GPU 温度", "GPU Temp"), "°C", 1, GpuColors.Rose, mapGpu { it.temp.toDoubleOrNull() }),
            TrendMetricSpec(s.t("GPU 热点温度", "GPU Hotspot"), "°C", 1, GpuColors.Crimson, mapGpu { it.hotspot.toDoubleOrNull() }),
            TrendMetricSpec(s.t("热点温差 (热点-GPU)", "Hotspot Delta"), "°C", 1, GpuColors.Orange, mapGpu { it.hotspotDeltaVal }),
            TrendMetricSpec(s.t("功耗", "Power"), "W", 1, GpuColors.Warn, mapGpu { it.power.toDoubleOrNull() }),
            TrendMetricSpec(s.t("核心频率", "Core Clock"), "MHz", 0, GpuColors.Accent, mapGpu { it.core.toDoubleOrNull() }),
            TrendMetricSpec(s.t("显存频率", "Mem Clock"), "MHz", 0, GpuColors.Violet, mapGpu { it.memclk.toDoubleOrNull() }),
            TrendMetricSpec(s.t("GPU 占用", "GPU Util"), "%", 0, GpuColors.Emerald, mapGpu { it.util.toDoubleOrNull() }),
            TrendMetricSpec(s.t("显存占用", "VRAM Used"), "MB", 0, GpuColors.Teal, mapGpu { it.vram.toDoubleOrNull() }),
            TrendMetricSpec(s.t("风扇转速", "Fan Speed"), "%", 0, GpuColors.Pink, mapGpu { it.fan.toDoubleOrNull() }),
            TrendMetricSpec(
                s.t("PCIe 代数", "PCIe Gen"),
                "Gen",
                0,
                GpuColors.BadLight,
                mapGpu { Regex("^Gen(\\d+)").find(it.pcie)?.groupValues?.getOrNull(1)?.toDoubleOrNull() },
            ),
            TrendMetricSpec(
                s.t("系统内存提交率", "Commit Charge"),
                "%",
                1,
                GpuColors.EmeraldLight,
                slicedMem.map { TrendPoint(it.ts, it.commitPct) },
            ),
            TrendMetricSpec(
                s.t("非分页内核池", "Non-Paged Pool"),
                "MB",
                0,
                GpuColors.WarnLight,
                slicedMem.map { TrendPoint(it.ts, it.nonpagedMb) },
            ),
        )
    }

    Column(modifier = Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(s.t("时间窗口：", "Window:"), color = GpuColors.Dim, fontSize = 12.sp)
            listOf(15, 20, 30, 60).forEach { n ->
                val sel = windowSize == n
                Surface(
                    onClick = { onChangeWindow(n) },
                    shape = RoundedCornerShape(8.dp),
                    color = if (sel) GpuColors.AccentBg else GpuColors.Card,
                    border = BorderStroke(1.dp, if (sel) GpuColors.AccentBorder else GpuColors.Border),
                ) {
                    Text(
                        text = s.t("${n}点", "${n} pts"),
                        color = if (sel) GpuColors.Accent else GpuColors.Dim,
                        fontSize = 12.sp,
                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
                    )
                }
            }
        }

        BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
            val cols = when {
                maxWidth >= 820.dp -> 3
                maxWidth >= 560.dp -> 2
                else -> 1
            }
            val useCompact = cols >= 2
            LazyVerticalGrid(
                columns = GridCells.Fixed(cols),
                horizontalArrangement = Arrangement.spacedBy(if (useCompact) 6.dp else 8.dp),
                verticalArrangement = Arrangement.spacedBy(if (useCompact) 6.dp else 8.dp),
                modifier = Modifier.fillMaxSize(),
            ) {
                gridItems(specs, key = { it.name }) { spec ->
                    NativeTrendCard(spec = spec, compact = useCompact)
                }
            }
        }
    }
}

@Composable
internal fun LogListTab(
    entries: List<LogEntryDto>,
    onlyBad: Boolean,
    filter: String,
    onFilterChange: (String) -> Unit,
    emptyHint: String,
    onClickItem: (LogEntryDto) -> Unit,
) {
    val filtered = remember(entries, onlyBad, filter) {
        val kw = filter.trim().lowercase()
        entries.filter { e ->
            if (onlyBad && e.sev == "normal") return@filter false
            if (kw.isNotEmpty()) {
                val hay = "${e.time} ${e.level} ${e.source} ${e.message} ${e.raw}".lowercase()
                if (!hay.contains(kw)) return@filter false
            }
            true
        }.asReversed()
    }

    Column(modifier = Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(
            value = filter,
            onValueChange = onFilterChange,
            singleLine = true,
            placeholder = {
                Text(
                    LocalStrings.current.t("输入关键字过滤（如 4101 / nvlddmkm）", "Filter by keyword (e.g. 4101 / nvlddmkm)"),
                    color = GpuColors.Dim,
                    fontSize = 12.sp,
                )
            },
            shape = RoundedCornerShape(10.dp),
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = GpuColors.Accent,
                unfocusedBorderColor = GpuColors.Border,
                focusedContainerColor = GpuColors.InputBg,
                unfocusedContainerColor = GpuColors.InputBg,
                focusedTextColor = Color.White,
                unfocusedTextColor = Color.White,
            ),
            modifier = Modifier.fillMaxWidth(),
        )

        if (filtered.isEmpty()) {
            EmptyPlaceholder(emptyHint)
        } else {
            BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
                val cols = if (maxWidth >= 720.dp) 2 else 1
                LazyVerticalGrid(
                    columns = GridCells.Fixed(cols),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                    modifier = Modifier.fillMaxSize(),
                ) {
                    gridItems(filtered) { e ->
                        LogEntryCard(entry = e, onClick = { onClickItem(e) })
                    }
                }
            }
        }
    }
}

@Composable
internal fun AlertsAndIncidentsTab(
    uiState: MainUiState,
    vm: MainViewModel,
    onClickLog: (LogEntryDto) -> Unit,
) {
    val s = LocalStrings.current
    val alerts = uiState.monitorState.alerts.asReversed()
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        item {
            Text(
                text = s.t("触发告警 (${alerts.size})", "Triggered Alerts (${alerts.size})"),
                color = Color.White,
                fontSize = 13.5.sp,
                fontWeight = FontWeight.Bold,
            )
        }
        if (alerts.isEmpty()) {
            item {
                Text(s.t("暂无告警记录", "No alert records"), color = GpuColors.Dim, fontSize = 12.sp)
            }
        } else {
            items(alerts.take(80)) { a ->
                LogEntryCard(entry = a, onClick = { onClickLog(a) })
            }
        }

        item {
            Spacer(modifier = Modifier.height(6.dp))
            Text(
                text = s.t("事故取证包 (${uiState.incidents.size})", "Incident Bundles (${uiState.incidents.size})"),
                color = Color.White,
                fontSize = 13.5.sp,
                fontWeight = FontWeight.Bold,
            )
        }
        if (uiState.incidents.isEmpty()) {
            item {
                Text(s.t("暂无事故取证记录", "No incident records"), color = GpuColors.Dim, fontSize = 12.sp)
            }
        } else {
            items(uiState.incidents) { inc ->
                val sel = inc.name == uiState.selectedIncident
                Surface(
                    onClick = { vm.selectIncident(inc.name) },
                    shape = RoundedCornerShape(10.dp),
                    color = if (sel) GpuColors.AccentBg else GpuColors.Card,
                    border = BorderStroke(1.dp, if (sel) GpuColors.AccentBorder else GpuColors.Border),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.padding(10.dp),
                    ) {
                        Text(
                            text = "${if (inc.belongsToSession) s.t("[本次] ", "[Cur] ") else ""}${inc.name}",
                            color = Color.White,
                            fontSize = 12.sp,
                            fontFamily = FontFamily.Monospace,
                        )
                        Spacer(modifier = Modifier.width(8.dp))
                        Text(
                            text = inc.reason,
                            color = GpuColors.Dim,
                            fontSize = 11.5.sp,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
            item {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    modifier = Modifier.horizontalScroll(rememberScrollState()),
                ) {
                    listOf("incident.json", "summary.txt", "nvidia-smi-q.txt", "Report_.wer").forEach { fn ->
                        val sel = uiState.previewFile == fn
                        Surface(
                            onClick = { vm.selectPreviewFile(fn) },
                            shape = RoundedCornerShape(8.dp),
                            color = if (sel) GpuColors.AccentBg else GpuColors.Card,
                            border = BorderStroke(1.dp, if (sel) GpuColors.AccentBorder else GpuColors.Border),
                        ) {
                            Text(
                                text = fn,
                                color = if (sel) GpuColors.Accent else GpuColors.Dim,
                                fontSize = 11.5.sp,
                                modifier = Modifier.padding(horizontal = 9.dp, vertical = 5.dp),
                            )
                        }
                    }
                }
                Spacer(modifier = Modifier.height(6.dp))
                Surface(
                    shape = RoundedCornerShape(10.dp),
                    color = GpuColors.InputBg,
                    border = BorderStroke(1.dp, GpuColors.Border),
                    modifier = Modifier.fillMaxWidth().heightIn(min = 120.dp, max = 260.dp),
                ) {
                    Text(
                        text = uiState.previewText.ifEmpty { s.t("(暂无预览内容)", "(No preview)") },
                        color = GpuColors.Text,
                        fontSize = 11.sp,
                        fontFamily = FontFamily.Monospace,
                        modifier = Modifier.verticalScroll(rememberScrollState()).padding(10.dp),
                    )
                }
            }
        }
    }
}

@Composable
private fun LogEntryCard(entry: LogEntryDto, onClick: () -> Unit) {
    Surface(
        onClick = onClick,
        shape = RoundedCornerShape(10.dp),
        color = GpuColors.Card,
        border = BorderStroke(1.dp, GpuColors.Border),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(modifier = Modifier.padding(10.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text(
                    text = entry.level,
                    color = sevTextColor(entry.sev),
                    fontSize = 11.sp,
                    fontWeight = FontWeight.Bold,
                )
                Text(
                    text = entry.source,
                    color = GpuColors.Accent,
                    fontSize = 11.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Text(
                    text = entry.time,
                    color = GpuColors.Dim,
                    fontSize = 10.5.sp,
                    fontFamily = FontFamily.Monospace,
                )
            }
            Spacer(modifier = Modifier.height(4.dp))
            Text(
                text = entry.message,
                color = GpuColors.Text,
                fontSize = 12.sp,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun RemoteSettingsDialog(
    cfg: ServerConfigDto,
    gpus: List<GpuDeviceDto>,
    appLang: AppLang,
    onChangeAppLang: (AppLang) -> Unit,
    onUpdate: (ServerConfigDto) -> Unit,
    onDismiss: () -> Unit,
    onSave: () -> Unit,
) {
    val s = LocalStrings.current
    var gamesText by remember(cfg) { mutableStateOf(cfg.gameNames.joinToString(", ")) }
    var tempWarn by remember(cfg) { mutableStateOf(cfg.tempWarn.toString()) }
    var hsWarn by remember(cfg) { mutableStateOf(cfg.hotspotWarn.toString()) }
    var deltaWarn by remember(cfg) { mutableStateOf(cfg.hotspotDeltaWarn.toString()) }
    var powerWarn by remember(cfg) { mutableStateOf(cfg.powerWarn.toString()) }
    var cpuTempWarn by remember(cfg) { mutableStateOf(cfg.cpuTempWarn.toString()) }
    var cpuUsageWarn by remember(cfg) { mutableStateOf(cfg.cpuUsageWarn.toString()) }
    var vramWarnPct by remember(cfg) { mutableStateOf(cfg.vramWarnPct.toString()) }
    var pcieDropWarn by remember(cfg) { mutableStateOf(cfg.pcieDropWarn) }
    var webPort by remember(cfg) { mutableStateOf(cfg.webPort.toString()) }
    var authPin by remember(cfg) { mutableStateOf(cfg.authPin) }
    var readOnlyRemote by remember(cfg) { mutableStateOf(cfg.readOnlyRemote) }

    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = GpuColors.Card,
        title = { Text(s.t("电脑端采集与界面设置", "Host & App Settings"), color = Color.White, fontWeight = FontWeight.Bold, fontSize = 16.sp) },
        text = {
            Column(
                modifier = Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(s.t("界面语言", "Language"), color = GpuColors.Text, fontSize = 12.sp)
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        listOf(AppLang.EN to "English", AppLang.ZH_CN to "简体中文").forEach { (l, label) ->
                            val active = appLang == l
                            Surface(
                                onClick = { onChangeAppLang(l) },
                                shape = RoundedCornerShape(8.dp),
                                color = if (active) GpuColors.AccentBg else GpuColors.InputBg,
                                border = BorderStroke(1.dp, if (active) GpuColors.Accent else GpuColors.Border),
                            ) {
                                Text(
                                    text = label,
                                    color = if (active) GpuColors.Accent else GpuColors.Dim,
                                    fontSize = 11.5.sp,
                                    fontWeight = if (active) FontWeight.Bold else FontWeight.Normal,
                                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
                                )
                            }
                        }
                    }
                }
                if (gpus.isNotEmpty()) {
                    Text(
                        text = s.t(
                            "当前显卡：#${cfg.gpuIndex} ${gpus.find { it.index == cfg.gpuIndex }?.name ?: ""}",
                            "Active GPU: #${cfg.gpuIndex} ${gpus.find { it.index == cfg.gpuIndex }?.name ?: ""}",
                        ),
                        color = GpuColors.Accent,
                        fontSize = 12.sp,
                    )
                }
                SettingField(s.t("监控游戏进程（逗号分隔）", "Watched Processes (comma-separated)"), gamesText) { gamesText = it }
                SettingField(s.t("温度告警阈值 (°C)", "GPU Temp Warn (°C)"), tempWarn) { tempWarn = it }
                SettingField(s.t("热点告警阈值 (°C)", "Hotspot Warn (°C)"), hsWarn) { hsWarn = it }
                SettingField(s.t("温差告警阈值 (°C)", "Delta Warn (°C)"), deltaWarn) { deltaWarn = it }
                SettingField(s.t("功耗告警阈值 (W)", "Power Warn (W)"), powerWarn) { powerWarn = it }
                SettingField(s.t("CPU 温度告警阈值 (°C)", "CPU Temp Warn (°C)"), cpuTempWarn) { cpuTempWarn = it }
                SettingField(s.t("CPU 占用告警阈值 (%)", "CPU Usage Warn (%)"), cpuUsageWarn) { cpuUsageWarn = it }
                SettingField(s.t("显存占用告警阈值 (%)", "VRAM Usage Warn (%)"), vramWarnPct) { vramWarnPct = it }
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(s.t("开启 PCIe 掉速预警", "PCIe Drop Alert"), color = GpuColors.Text, fontSize = 12.sp)
                    androidx.compose.material3.Switch(
                        checked = pcieDropWarn,
                        onCheckedChange = { pcieDropWarn = it },
                    )
                }
                SettingField(s.t("网页服务端口", "Web Port"), webPort) { webPort = it }
                SettingField(s.t("远程访问口令（留空不限制）", "Remote PIN (blank = off)"), authPin) { authPin = it }
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(s.t("远程只读访客模式", "Read-Only Remote Mode"), color = GpuColors.Text, fontSize = 12.sp)
                    androidx.compose.material3.Switch(
                        checked = readOnlyRemote,
                        onCheckedChange = { readOnlyRemote = it },
                    )
                }
            }
        },
        confirmButton = {
            Button(
                onClick = {
                    val next = cfg.copy(
                        gameNames = gamesText.split(",").map { it.trim() }.filter { it.isNotEmpty() },
                        tempWarn = tempWarn.toDoubleOrNull() ?: cfg.tempWarn,
                        hotspotWarn = hsWarn.toDoubleOrNull() ?: cfg.hotspotWarn,
                        hotspotDeltaWarn = deltaWarn.toDoubleOrNull() ?: cfg.hotspotDeltaWarn,
                        powerWarn = powerWarn.toDoubleOrNull() ?: cfg.powerWarn,
                        cpuTempWarn = cpuTempWarn.toDoubleOrNull() ?: cfg.cpuTempWarn,
                        cpuUsageWarn = cpuUsageWarn.toDoubleOrNull() ?: cfg.cpuUsageWarn,
                        vramWarnPct = vramWarnPct.toDoubleOrNull() ?: cfg.vramWarnPct,
                        pcieDropWarn = pcieDropWarn,
                        webPort = (webPort.toIntOrNull() ?: cfg.webPort).coerceIn(1024, 65535),
                        authPin = authPin.trim(),
                        readOnlyRemote = readOnlyRemote,
                    )
                    onUpdate(next)
                    onSave()
                },
                colors = ButtonDefaults.buttonColors(containerColor = GpuColors.Emerald, contentColor = GpuColors.EmeraldText),
            ) { Text(s.t("保存设置", "Save Settings"), fontWeight = FontWeight.Bold) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(s.cancelBtn, color = GpuColors.Dim) }
        },
    )
}

@Composable
private fun SettingField(label: String, value: String, onChange: (String) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(label, color = GpuColors.Dim, fontSize = 11.5.sp)
        OutlinedTextField(
            value = value,
            onValueChange = onChange,
            singleLine = true,
            shape = RoundedCornerShape(9.dp),
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = GpuColors.Accent,
                unfocusedBorderColor = GpuColors.Border,
                focusedContainerColor = GpuColors.InputBg,
                unfocusedContainerColor = GpuColors.InputBg,
                focusedTextColor = Color.White,
                unfocusedTextColor = Color.White,
            ),
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

@Composable
internal fun SmallActionChip(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    label: String?,
    fg: Color,
    bg: Color,
    border: Color? = null,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    Surface(
        onClick = onClick,
        shape = RoundedCornerShape(7.dp),
        color = bg,
        border = border?.let { BorderStroke(1.dp, it) },
        modifier = modifier,
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.Center,
            modifier = Modifier.padding(horizontal = if (label == null) 7.dp else 8.dp, vertical = 5.dp),
        ) {
            Icon(imageVector = icon, contentDescription = label, tint = fg, modifier = Modifier.size(14.dp))
            if (label != null) {
                Spacer(modifier = Modifier.width(3.dp))
                Text(text = label, color = fg, fontSize = 11.sp)
            }
        }
    }
}

@Composable
private fun BigHudDashboard(
    data: StateResponse,
    latestGpu: GpuRowDto?,
    onExit: () -> Unit,
) {
    val s = LocalStrings.current
    Column(
        verticalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState()),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text(
                text = s.t("远距离副屏大字仪表盘", "Distance HUD Dashboard"),
                color = GpuColors.Dim,
                fontSize = 13.sp,
                fontWeight = FontWeight.SemiBold,
            )
            SmallActionChip(
                icon = Icons.Rounded.OpenInFull,
                label = s.t("退出大字", "Exit HUD"),
                fg = GpuColors.Accent,
                bg = GpuColors.AccentBg,
                border = GpuColors.AccentBorder,
                onClick = onExit,
            )
        }

        Row(
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Surface(
                shape = RoundedCornerShape(16.dp),
                color = GpuColors.Card,
                border = BorderStroke(1.dp, GpuColors.Border),
                modifier = Modifier.weight(1f),
            ) {
                Column(
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                    modifier = Modifier.padding(18.dp),
                ) {
                    Text(s.t("GPU 核心状态", "GPU Core Status"), color = GpuColors.Dim, fontSize = 14.sp)
                    Text(
                        text = latestGpu?.let { "${it.temp}°C" } ?: "—",
                        color = Color.White,
                        fontSize = 44.sp,
                        fontWeight = FontWeight.Bold,
                        fontFamily = FontFamily.Monospace,
                    )
                    Text(
                        text = s.t(
                            "热点 ${latestGpu?.hotspot ?: "—"}°C · 功耗 ${latestGpu?.power ?: "—"} W",
                            "Hotspot ${latestGpu?.hotspot ?: "—"}°C · Power ${latestGpu?.power ?: "—"} W",
                        ),
                        color = GpuColors.WarnLight,
                        fontSize = 18.sp,
                        fontFamily = FontFamily.Monospace,
                    )
                    Text(
                        text = s.t(
                            "核心 ${latestGpu?.core ?: "—"} MHz · 占用 ${latestGpu?.util ?: "—"}% · 降频 ${latestGpu?.thrLabel?.ifBlank { "无" } ?: "无"}",
                            "Core ${latestGpu?.core ?: "—"} MHz · Util ${latestGpu?.util ?: "—"}% · Thr ${s.trThr(latestGpu?.thrLabel?.ifBlank { "None" } ?: "None")}",
                        ),
                        color = GpuColors.Accent,
                        fontSize = 14.sp,
                        fontFamily = FontFamily.Monospace,
                    )
                    data.stats.gpuTemp?.let { st ->
                        Text(
                            text = s.t(
                                "温度极值 ${st.min}~${st.max}°C (均 ${st.avg}°C)",
                                "Temp Range ${st.min}~${st.max}°C (avg ${st.avg}°C)",
                            ),
                            color = GpuColors.Dim,
                            fontSize = 13.sp,
                            fontFamily = FontFamily.Monospace,
                        )
                    }
                }
            }

            data.cpu?.let { cpu ->
                Surface(
                    shape = RoundedCornerShape(16.dp),
                    color = GpuColors.Card,
                    border = BorderStroke(1.dp, GpuColors.Border),
                    modifier = Modifier.weight(1f),
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(18.dp),
                        modifier = Modifier.padding(18.dp),
                    ) {
                        CpuRingGauge(
                            progress = (cpu.totalUsage / 100.0).toFloat(),
                            color = Color(0xFF38BDF8),
                            size = 110.dp,
                            strokeWidth = 10.dp,
                            centerText = "${cpu.totalUsage.toInt()}%",
                            fontSize = 24.sp,
                        )
                        Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            Text(s.t("CPU 实时状态", "CPU Live Status"), color = GpuColors.Dim, fontSize = 14.sp)
                            Text(
                                text = cpu.tempC?.let { String.format(Locale.US, "%.1f°C", it) } ?: "—",
                                color = Color.White,
                                fontSize = 36.sp,
                                fontWeight = FontWeight.Bold,
                                fontFamily = FontFamily.Monospace,
                            )
                            Text(
                                text = s.t(
                                    "均频 ${cpu.avgFreqMhz} MHz · 峰值 ${cpu.maxFreqMhz} MHz",
                                    "Avg ${cpu.avgFreqMhz} MHz · Max ${cpu.maxFreqMhz} MHz",
                                ),
                                color = GpuColors.Text,
                                fontSize = 15.sp,
                                fontFamily = FontFamily.Monospace,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun EmptyPlaceholder(msg: String) {
    Box(contentAlignment = Alignment.Center, modifier = Modifier.fillMaxSize()) {
        Text(text = msg, color = GpuColors.Dim, fontSize = 13.sp)
    }
}

internal fun sevTextColor(sev: String): Color = when (sev) {
    "bad" -> GpuColors.BadLight
    "warn" -> GpuColors.WarnLight
    else -> GpuColors.Text
}

private fun exportZipViaDownloadManager(
    context: Context,
    baseUrl: String,
    session: String,
    strings: AppStrings,
    onToast: (String) -> Unit,
) {
    try {
        val enc = URLEncoder.encode(session, "UTF-8")
        val dlUrl = "$baseUrl/api/sessions/export?session=$enc"
        val fileName = "gpucap-$session.zip"
        val req = DownloadManager.Request(Uri.parse(dlUrl))
        req.setTitle(fileName)
        req.setDescription(strings.t("GPUCap 会话取证导出", "GPUCap Session Export"))
        req.setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
        req.setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, fileName)
        val dm = context.getSystemService(Context.DOWNLOAD_SERVICE) as? DownloadManager
        if (dm != null) {
            dm.enqueue(req)
            onToast(strings.t("开始下载：$fileName", "Downloading: $fileName"))
        }
    } catch (e: Exception) {
        onToast(strings.t("导出失败：${e.message}", "Export failed: ${e.message}"))
    }
}
