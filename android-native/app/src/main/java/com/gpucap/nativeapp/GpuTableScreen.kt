package com.gpucap.nativeapp

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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.Pause
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.WarningAmber
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * 独立的全屏数据明细分页界面：
 * 将实时 GPU 表格、趋势曲线、系统事件、WER 崩溃与告警取证统一放在全屏分页中展示。
 */
@Composable
fun GpuTableScreen(
    uiState: MainUiState,
    vm: MainViewModel,
) {
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
        // 顶部返回与快捷控制栏（紧凑 2 行）
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(GpuColors.TopBar)
                .padding(horizontal = 10.dp, vertical = 6.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
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
                            contentDescription = "返回",
                            tint = GpuColors.Text,
                            modifier = Modifier.size(15.dp),
                        )
                        Spacer(modifier = Modifier.width(2.dp))
                        Text(text = "返回", color = GpuColors.Text, fontSize = 11.5.sp)
                    }
                }

                Text(
                    text = when (uiState.selectedSession) {
                        "" -> "数据明细"
                        "__live__" -> "实时监测(仅内存)"
                        else -> uiState.selectedSession
                    },
                    color = Color.White,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.SemiBold,
                    fontFamily = FontFamily.Monospace,
                    maxLines = 1,
                    modifier = Modifier.weight(1f),
                )

                SmallActionChip(
                    icon = if (uiState.paused) Icons.Rounded.PlayArrow else Icons.Rounded.Pause,
                    label = if (uiState.paused) "继续" else "暂停",
                    fg = if (uiState.paused) GpuColors.WarnLight else GpuColors.Text,
                    bg = if (uiState.paused) GpuColors.WarnBg else GpuColors.Card,
                    border = if (uiState.paused) GpuColors.WarnBorder else GpuColors.Border,
                    onClick = vm::togglePaused,
                )

                SmallActionChip(
                    icon = Icons.Rounded.WarningAmber,
                    label = "异常",
                    fg = if (uiState.onlyBad) GpuColors.BadLight else GpuColors.Text,
                    bg = if (uiState.onlyBad) GpuColors.BadBg else GpuColors.Card,
                    border = if (uiState.onlyBad) GpuColors.BadBorder else GpuColors.Border,
                    onClick = vm::toggleOnlyBad,
                )

                if (!uiState.fullHistory) {
                    SmallActionChip(
                        icon = Icons.Rounded.Refresh,
                        label = "全部",
                        fg = GpuColors.Text,
                        bg = GpuColors.Card,
                        border = GpuColors.Border,
                        onClick = vm::loadFullHistory,
                    )
                }
            }

            // 5 大分页切换栏
            MonitorTabBar(
                activeTab = uiState.activeTab,
                data = data,
                onSelectTab = vm::setTab,
            )
        }

        // 全屏分页内容区
        Box(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(horizontal = 10.dp, vertical = 8.dp),
        ) {
            when (uiState.activeTab) {
                MonitorTab.Gpu -> FullGpuTableContent(
                    rows = data.gpuRows,
                    onlyBad = uiState.onlyBad,
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
                    emptyHint = "暂无系统事件记录",
                    onClickItem = { logDetail = it },
                )

                MonitorTab.Wer -> LogListTab(
                    entries = data.wer,
                    onlyBad = false,
                    filter = uiState.logFilter,
                    onFilterChange = vm::setLogFilter,
                    emptyHint = "暂未监测到 WER 崩溃与转储记录",
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
    val pairs = listOf(
        "采样时间" to row.ts,
        "GPU 温度" to "${row.temp} °C",
        "热点温度" to if (row.hotspot.isEmpty()) "—" else "${row.hotspot} °C",
        "热点温差" to (row.hotspotDeltaText?.let { "$it °C" } ?: "—"),
        "实时功耗" to "${row.power} W",
        "核心频率" to "${row.core} MHz",
        "显存频率" to "${row.memclk} MHz",
        "PCIe 链路" to row.pcie,
        "GPU 占用" to "${row.util} %",
        "显存占用" to "${row.vram} MB",
        "风扇转速" to "${row.fan} %",
        "降频原因" to (row.thrLabel.ifBlank { row.thr }.ifBlank { "—" }),
        "状态判定" to row.ok,
    )
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = GpuColors.Card,
        title = {
            Text(
                text = "采样详情 · ${row.ts}",
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
                Text("关闭", color = GpuColors.Accent, fontWeight = FontWeight.Bold)
            }
        },
    )
}

@Composable
internal fun FullGpuTableContent(
    rows: List<GpuRowDto>,
    onlyBad: Boolean,
    onClickRow: (GpuRowDto) -> Unit,
) {
    val list = remember(rows, onlyBad) {
        val base = if (onlyBad) rows.filter { it.sev != "normal" } else rows
        base.asReversed()
    }
    if (list.isEmpty()) {
        Box(contentAlignment = Alignment.Center, modifier = Modifier.fillMaxSize()) {
            Text(text = "暂无显卡采样记录", color = GpuColors.Dim, fontSize = 13.sp)
        }
        return
    }

    val hScroll = rememberScrollState()
    val timeWidth = 80.dp
    val baseCols: List<Pair<String, Dp>> = listOf(
        "温度" to 56.dp,
        "热点" to 60.dp,
        "温差" to 56.dp,
        "功耗" to 64.dp,
        "核心" to 62.dp,
        "显存频" to 64.dp,
        "PCIe" to 72.dp,
        "占用" to 52.dp,
        "显存" to 68.dp,
        "风扇" to 52.dp,
        "降频位" to 82.dp,
        "状态" to 50.dp,
    )
    val baseTotal = 738.dp

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
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(GpuColors.HeadBg)
                        .padding(vertical = 9.dp),
                ) {
                    Text(
                        text = "时间",
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

                LazyColumn(modifier = Modifier.fillMaxSize()) {
                    itemsIndexed(list) { index, r ->
                        val fg = sevTextColor(r.sev)
                        val vals = listOf(
                            r.temp,
                            r.hotspot.ifEmpty { "—" },
                            r.hotspotDeltaText ?: "—",
                            r.power,
                            r.core,
                            r.memclk,
                            r.pcie,
                            r.util,
                            r.vram,
                            r.fan,
                            r.thrLabel.ifBlank { r.thr }.ifBlank { "—" },
                            r.ok,
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
        }
    }
}
