package com.gpucap.nativeapp

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.ArrowUpward
import androidx.compose.material.icons.rounded.Pause
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material.icons.rounded.Visibility
import androidx.compose.material.icons.rounded.VisibilityOff
import androidx.compose.material.icons.rounded.WarningAmber
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch

/**
 * 独立的全屏数据明细分页界面：
 * 将实时 GPU 表格、趋势曲线、系统事件、WER 崩溃与告警取证统一放在全屏分页中展示。
 */
@Composable
fun GpuTableScreen(
    uiState: MainUiState,
    vm: MainViewModel,
) {
    val s = LocalStrings.current
    val data = uiState.monitorState
    var detailRow by remember { mutableStateOf<GpuRowDto?>(null) }
    var logDetail by remember { mutableStateOf<LogEntryDto?>(null) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(GpuColors.Bg)
            .statusBarsPadding()
            .navigationBarsPadding(),
    ) {
        // 顶部返回与快捷控制栏（紧凑化，支持实时监测、常亮与异常过滤）
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(GpuColors.TopBar)
                .padding(horizontal = 10.dp, vertical = 5.dp),
            verticalArrangement = Arrangement.spacedBy(5.dp),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(5.dp),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Surface(
                    onClick = vm::closeGpuTable,
                    shape = RoundedCornerShape(7.dp),
                    color = GpuColors.SurfaceBtn,
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.padding(horizontal = 7.dp, vertical = 5.dp),
                    ) {
                        Icon(
                            imageVector = Icons.AutoMirrored.Rounded.ArrowBack,
                            contentDescription = s.t("返回", "Back"),
                            tint = GpuColors.Text,
                            modifier = Modifier.size(15.dp),
                        )
                        Spacer(modifier = Modifier.width(2.dp))
                        Text(text = s.t("返回", "Back"), color = GpuColors.Text, fontSize = 11.5.sp)
                    }
                }

                Text(
                    text = when (uiState.selectedSession) {
                        "" -> s.t("数据明细", "Telemetry Details")
                        "__live__" -> s.t("实时监测(仅内存)", "Live (Memory)")
                        else -> uiState.selectedSession
                    },
                    color = Color.White,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.SemiBold,
                    fontFamily = FontFamily.Monospace,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )

                SmallActionChip(
                    icon = if (data.liveRunning) Icons.Rounded.Stop else Icons.Rounded.PlayArrow,
                    label = if (data.liveRunning) s.stopCap else s.t("实时", "Live"),
                    fg = if (data.liveRunning) GpuColors.BadLight else GpuColors.Accent,
                    bg = if (data.liveRunning) GpuColors.BadBg else GpuColors.AccentBg,
                    border = if (data.liveRunning) GpuColors.BadBorder else GpuColors.AccentBorder,
                    onClick = vm::toggleLiveMonitor,
                )

                SmallActionChip(
                    icon = if (uiState.keepScreenOn) Icons.Rounded.Visibility else Icons.Rounded.VisibilityOff,
                    label = s.t("常亮", "Awake"),
                    fg = if (uiState.keepScreenOn) GpuColors.EmeraldLight else GpuColors.Dim,
                    bg = if (uiState.keepScreenOn) GpuColors.EmeraldBg else GpuColors.Card,
                    border = if (uiState.keepScreenOn) GpuColors.EmeraldBorder else GpuColors.Border,
                    onClick = vm::toggleKeepScreenOn,
                )

                SmallActionChip(
                    icon = if (uiState.paused) Icons.Rounded.PlayArrow else Icons.Rounded.Pause,
                    label = if (uiState.paused) s.resumeUi else s.pauseUi,
                    fg = if (uiState.paused) GpuColors.WarnLight else GpuColors.Text,
                    bg = if (uiState.paused) GpuColors.WarnBg else GpuColors.Card,
                    border = if (uiState.paused) GpuColors.WarnBorder else GpuColors.Border,
                    onClick = vm::togglePaused,
                )

                SmallActionChip(
                    icon = Icons.Rounded.WarningAmber,
                    label = s.t("异常", "Alerts"),
                    fg = if (uiState.onlyBad) GpuColors.BadLight else GpuColors.Text,
                    bg = if (uiState.onlyBad) GpuColors.BadBg else GpuColors.Card,
                    border = if (uiState.onlyBad) GpuColors.BadBorder else GpuColors.Border,
                    onClick = vm::toggleOnlyBad,
                )

                if (!uiState.fullHistory) {
                    SmallActionChip(
                        icon = Icons.Rounded.Refresh,
                        label = s.t("全部", "All"),
                        fg = GpuColors.Text,
                        bg = GpuColors.Card,
                        border = GpuColors.Border,
                        onClick = vm::loadFullHistory,
                    )
                }
            }

            // 5/6 大分页切换栏
            MonitorTabBar(
                activeTab = uiState.activeTab,
                data = data,
                onSelectTab = vm::setTab,
            )

            // 全屏分页顶部实时告警横幅
            uiState.liveAlertBanner?.let { banner ->
                Surface(
                    shape = RoundedCornerShape(8.dp),
                    color = GpuColors.BadBg,
                    border = BorderStroke(1.dp, GpuColors.BadBorder),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
                    ) {
                        Text(
                            text = s.t("实时告警：$banner", "Live Alert: $banner"),
                            color = GpuColors.BadLight,
                            fontSize = 11.sp,
                            fontWeight = FontWeight.SemiBold,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f),
                        )
                        Text(
                            text = s.closeBtn,
                            color = Color.White,
                            fontSize = 10.5.sp,
                            fontWeight = FontWeight.Bold,
                            modifier = Modifier.clickable(onClick = vm::dismissLiveAlertBanner),
                        )
                    }
                }
            }
        }

        // 全屏分页内容区（带平滑切页过渡）
        AnimatedContent(
            targetState = uiState.activeTab,
            transitionSpec = { fadeIn() togetherWith fadeOut() },
            label = "full_tab_transition",
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(horizontal = 10.dp, vertical = 6.dp),
        ) { tab ->
            when (tab) {
                MonitorTab.Gpu -> FullGpuTableContent(
                    rows = data.gpuRows,
                    onlyBad = uiState.onlyBad,
                    stats = data.stats,
                    onClickRow = { detailRow = it },
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
                    emptyHint = s.t("暂无系统事件记录", "No system events recorded"),
                    onClickItem = { logDetail = it },
                )

                MonitorTab.Wer -> LogListTab(
                    entries = data.wer,
                    onlyBad = false,
                    filter = uiState.logFilter,
                    onFilterChange = vm::setLogFilter,
                    emptyHint = s.t("暂未监测到 WER 崩溃与转储记录", "No WER crash or dump records"),
                    onClickItem = { logDetail = it },
                )

                MonitorTab.Alerts -> AlertsAndIncidentsTab(
                    uiState = uiState,
                    vm = vm,
                    onClickLog = { logDetail = it },
                )
            }
        }
    }

    // GPU 采样行完整参数详情弹窗
    detailRow?.let { r ->
        GpuRowDetailDialog(row = r, onDismiss = { detailRow = null })
    }

    // 日志详情弹窗
    logDetail?.let { entry ->
        LogDetailDialog(
            entry = entry,
            onDismiss = { logDetail = null },
            onToast = vm::emitToast,
        )
    }
}

@Composable
internal fun GpuRowDetailDialog(row: GpuRowDto, onDismiss: () -> Unit) {
    val s = LocalStrings.current
    val pairs = listOf(
        s.t("采样时间", "Timestamp") to row.ts,
        s.t("GPU 温度", "GPU Temp") to "${row.temp} °C",
        s.t("热点温度", "Hotspot") to if (row.hotspot.isEmpty()) "—" else "${row.hotspot} °C",
        s.t("热点温差", "Delta") to (row.hotspotDeltaText?.let { "$it °C" } ?: "—"),
        s.t("实时功耗", "Power") to "${row.power} W",
        s.t("核心频率", "Core Clock") to "${row.core} MHz",
        s.t("显存频率", "Mem Clock") to "${row.memclk} MHz",
        s.t("PCIe 链路", "PCIe Link") to if (row.pcieDrop) s.t("${row.pcie} (降速)", "${row.pcie} (Drop)") else row.pcie,
        s.t("GPU 占用", "GPU Util") to "${row.util} %",
        s.t("显存占用", "VRAM") to if (row.vramPct > 0.0) "${row.vram}/${row.vramTotal} MB (${row.vramPct}%)" else "${row.vram} MB",
        s.t("风扇转速", "Fan Speed") to "${row.fan} %",
        s.t("降频原因", "Throttle") to (s.trThr(row.thrLabel.ifBlank { row.thr }).ifBlank { "—" }),
        s.t("状态判定", "Status") to s.trBackend(row.ok),
    )
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = GpuColors.Card,
        title = {
            Text(
                text = s.t("采样详情 · ${row.ts}", "Sample Detail · ${row.ts}"),
                color = Color.White,
                fontSize = 15.sp,
                fontWeight = FontWeight.Bold,
                fontFamily = FontFamily.Monospace,
            )
        },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(5.dp)) {
                pairs.forEach { (k, v) ->
                    Row(
                        horizontalArrangement = Arrangement.SpaceBetween,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(text = k, color = GpuColors.Dim, fontSize = 12.sp)
                        Text(
                            text = v,
                            color = Color.White,
                            fontSize = 12.sp,
                            fontFamily = FontFamily.Monospace,
                            fontWeight = FontWeight.Medium,
                        )
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) {
                Text(s.closeBtn, color = GpuColors.Accent, fontWeight = FontWeight.Bold)
            }
        },
    )
}

@Composable
internal fun FullGpuTableContent(
    rows: List<GpuRowDto>,
    onlyBad: Boolean,
    stats: LiveStatsSummaryDto = LiveStatsSummaryDto(),
    onClickRow: (GpuRowDto) -> Unit,
) {
    val list = remember(rows, onlyBad) {
        val base = if (onlyBad) rows.filter { it.sev != "normal" } else rows
        base.asReversed()
    }
    val s = LocalStrings.current
    if (list.isEmpty()) {
        Box(contentAlignment = Alignment.Center, modifier = Modifier.fillMaxSize()) {
            Text(text = s.t("暂无显卡采样记录", "No GPU telemetry samples"), color = GpuColors.Dim, fontSize = 13.sp)
        }
        return
    }

    val hScroll = rememberScrollState()
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val showJumpTop by remember { derivedStateOf { listState.firstVisibleItemIndex > 3 } }
    val thrCount = remember(list) {
        list.count { it.thrLabel.isNotBlank() && it.thrLabel != "空闲" }
    }

    val timeWidth = 76.dp
    val baseCols: List<Pair<String, Dp>> = listOf(
        s.t("温度", "Temp") to 54.dp,
        s.t("热点", "Hotspot") to 58.dp,
        s.t("温差", "Delta") to 54.dp,
        s.t("功耗", "Power") to 62.dp,
        s.t("核心", "Core") to 60.dp,
        s.t("显存频", "MemClk") to 62.dp,
        "PCIe" to 78.dp,
        s.t("占用", "Util") to 50.dp,
        s.t("显存", "VRAM") to 88.dp,
        s.t("风扇", "Fan") to 50.dp,
        s.t("降频原因", "Throttle") to 96.dp,
        s.t("状态", "State") to 48.dp,
    )
    val baseTotal = 760.dp

    Surface(
        shape = RoundedCornerShape(12.dp),
        color = GpuColors.Card,
        border = BorderStroke(1.dp, GpuColors.Border),
        modifier = Modifier.fillMaxSize(),
    ) {
        BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
            val rightAvail = (maxWidth - timeWidth - 2.dp).coerceAtLeast(baseTotal)
            val scale = rightAvail / baseTotal
            val cols = baseCols.map { (t, w) -> t to (w * scale) }

            Column(modifier = Modifier.fillMaxSize()) {
                // 表格顶部极值与采样统计条
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(GpuColors.InputBg.copy(alpha = 0.7f))
                        .padding(horizontal = 10.dp, vertical = 5.dp),
                ) {
                    Text(
                        text = s.t(
                            "样本 ${list.size} 条 · 降频 $thrCount 次（点击行查看详情）",
                            "Samples: ${list.size} · Throttled: $thrCount (tap row for details)",
                        ),
                        color = GpuColors.Dim,
                        fontSize = 10.5.sp,
                        fontFamily = FontFamily.Monospace,
                    )
                    stats.gpuTemp?.let { st ->
                        val pStr = stats.gpuPower?.let { s.t(" · 功耗峰 ${it.max}W", " · Peak ${it.max}W") } ?: ""
                        Text(
                            text = s.t(
                                "温度 ${st.min}~${st.max}°C (均${st.avg}°C)$pStr",
                                "Temp ${st.min}~${st.max}°C (avg ${st.avg}°C)$pStr",
                            ),
                            color = GpuColors.Accent,
                            fontSize = 10.5.sp,
                            fontFamily = FontFamily.Monospace,
                        )
                    }
                }

                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(GpuColors.HeadBg)
                        .padding(vertical = 8.dp),
                ) {
                    Text(
                        text = s.t("时间", "Time"),
                        color = Color.White,
                        fontSize = 11.5.sp,
                        fontWeight = FontWeight.SemiBold,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.width(timeWidth),
                    )
                    Row(modifier = Modifier.weight(1f).horizontalScroll(hScroll)) {
                        cols.forEach { (title, w) ->
                            Text(
                                text = title,
                                color = GpuColors.Text,
                                fontSize = 11.5.sp,
                                fontWeight = FontWeight.SemiBold,
                                textAlign = TextAlign.Center,
                                modifier = Modifier.width(w),
                            )
                        }
                    }
                }

                HorizontalDivider(color = GpuColors.Border, thickness = 1.dp)

                LazyColumn(
                    state = listState,
                    modifier = Modifier.fillMaxSize(),
                ) {
                    itemsIndexed(list) { index, r ->
                        val fg = sevTextColor(r.sev)
                        val vals = listOf(
                            r.temp,
                            r.hotspot.ifEmpty { "—" },
                            r.hotspotDeltaText ?: "—",
                            r.power,
                            r.core,
                            r.memclk,
                            if (r.pcieDrop) "${r.pcie}⚠" else r.pcie,
                            r.util,
                            if (r.vramPct > 0.0) "${r.vram}(${r.vramPct.toInt()}%)" else r.vram,
                            r.fan,
                            r.thrLabel.ifBlank { r.thr }.ifBlank { "—" },
                            s.trBackend(r.ok),
                        )
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            modifier = Modifier
                                .fillMaxWidth()
                                .background(
                                    if (index % 2 == 1) GpuColors.InputBg.copy(alpha = 0.45f)
                                    else Color.Transparent,
                                )
                                .clickable { onClickRow(r) }
                                .padding(vertical = 8.dp),
                        ) {
                            Text(
                                text = r.ts,
                                color = fg,
                                fontSize = 11.5.sp,
                                fontFamily = FontFamily.Monospace,
                                textAlign = TextAlign.Center,
                                maxLines = 1,
                                modifier = Modifier.width(timeWidth),
                            )
                            Row(modifier = Modifier.weight(1f).horizontalScroll(hScroll)) {
                                vals.forEachIndexed { i, v ->
                                    Text(
                                        text = v,
                                        color = fg,
                                        fontSize = 11.5.sp,
                                        fontFamily = FontFamily.Monospace,
                                        textAlign = TextAlign.Center,
                                        maxLines = 1,
                                        modifier = Modifier.width(cols[i].second),
                                    )
                                }
                            }
                        }
                        HorizontalDivider(
                            color = GpuColors.Border.copy(alpha = 0.4f),
                            thickness = 0.5.dp,
                        )
                    }
                }
            }

            if (showJumpTop) {
                Surface(
                    onClick = { scope.launch { listState.animateScrollToItem(0) } },
                    shape = RoundedCornerShape(99.dp),
                    color = GpuColors.Accent,
                    shadowElevation = 4.dp,
                    modifier = Modifier
                        .align(Alignment.BottomEnd)
                        .padding(14.dp),
                ) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
                    ) {
                        Icon(
                            imageVector = Icons.Rounded.ArrowUpward,
                            contentDescription = s.t("回到最新", "Jump to Latest"),
                            tint = GpuColors.AccentDark,
                            modifier = Modifier.size(14.dp),
                        )
                        Text(
                            text = s.t("回到最新", "Latest"),
                            color = GpuColors.AccentDark,
                            fontSize = 11.5.sp,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }
            }
        }
    }
}
