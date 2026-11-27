package com.gpucap.nativeapp

import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Clear
import androidx.compose.material.icons.rounded.DeleteOutline
import androidx.compose.material.icons.rounded.Memory
import androidx.compose.material.icons.rounded.NetworkCheck
import androidx.compose.material.icons.rounded.QrCodeScanner
import androidx.compose.material.icons.rounded.RocketLaunch
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

@Composable
fun ConnectScreen(
    inputUrl: String,
    onInputChange: (String) -> Unit,
    historyUrls: List<String>,
    pingMap: Map<String, PingStatus>,
    discoveredHosts: List<DiscoveredHost>,
    discoveringHosts: Boolean,
    dualChoice: QrParsedResult.Dual?,
    onDismissDualChoice: () -> Unit,
    onConnectManual: () -> Unit,
    onStartQrScan: () -> Unit,
    onSelectHistory: (String) -> Unit,
    onRemoveHistory: (String) -> Unit,
    onPingAll: () -> Unit,
    onDiscoverHosts: () -> Unit,
    isZh: Boolean = false,
    onToggleLang: () -> Unit = {},
) {
    val s = LocalStrings.current
    BoxWithConstraints(
        contentAlignment = Alignment.TopCenter,
        modifier = Modifier
            .fillMaxSize()
            .background(GpuColors.Bg)
            .statusBarsPadding()
            .navigationBarsPadding(),
    ) {
        val wide = maxWidth >= 720.dp

        val connectAndQr: @Composable () -> Unit = {
            Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
                SectionCard {
                    Text(
                        text = s.connectHostTitle,
                        color = Color.White,
                        fontSize = 15.sp,
                        fontWeight = FontWeight.Bold,
                    )
                    Spacer(modifier = Modifier.height(12.dp))
                    OutlinedTextField(
                        value = inputUrl,
                        onValueChange = onInputChange,
                        singleLine = true,
                        placeholder = {
                            Text(
                                text = s.t("例如 192.168.1.100 或 [240e:...]:8787", "e.g. 192.168.1.100 or [240e:...]:8787"),
                                color = GpuColors.Dim,
                                fontSize = 13.sp,
                            )
                        },
                        trailingIcon = {
                            if (inputUrl.isNotEmpty()) {
                                IconButton(onClick = { onInputChange("") }) {
                                    Icon(
                                        imageVector = Icons.Rounded.Clear,
                                        contentDescription = s.t("清空", "Clear"),
                                        tint = GpuColors.Dim,
                                    )
                                }
                            }
                        },
                        keyboardOptions = KeyboardOptions(
                            keyboardType = KeyboardType.Uri,
                            imeAction = ImeAction.Go,
                        ),
                        keyboardActions = KeyboardActions(onGo = { onConnectManual() }),
                        shape = RoundedCornerShape(12.dp),
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
                    Spacer(modifier = Modifier.height(12.dp))
                    Button(
                        onClick = onConnectManual,
                        shape = RoundedCornerShape(12.dp),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = GpuColors.Emerald,
                            contentColor = GpuColors.EmeraldText,
                        ),
                        contentPadding = PaddingValues(vertical = 12.dp),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Icon(
                            imageVector = Icons.Rounded.RocketLaunch,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                        )
                        Spacer(modifier = Modifier.width(8.dp))
                        Text(
                            text = s.saveAndConnect,
                            fontSize = 14.sp,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }

                SectionCard {
                    Text(
                        text = s.t("扫码快速连接", "Quick QR Connect"),
                        color = Color.White,
                        fontSize = 15.sp,
                        fontWeight = FontWeight.Bold,
                    )
                    Spacer(modifier = Modifier.height(6.dp))
                    Text(
                        text = s.t(
                            "支持识别电脑/网页端二维码（同时包含 IPv4 与 IPv6 时可自由选择），也支持从相册选图识别",
                            "Scan desktop/web QR codes (choose IPv4 or IPv6 for dual-stack) or pick an image from gallery",
                        ),
                        color = GpuColors.Dim,
                        fontSize = 12.sp,
                        lineHeight = 17.sp,
                    )
                    Spacer(modifier = Modifier.height(12.dp))
                    Button(
                        onClick = onStartQrScan,
                        shape = RoundedCornerShape(12.dp),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = GpuColors.Accent,
                            contentColor = GpuColors.AccentDark,
                        ),
                        contentPadding = PaddingValues(vertical = 12.dp),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Icon(
                            imageVector = Icons.Rounded.QrCodeScanner,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                        )
                        Spacer(modifier = Modifier.width(8.dp))
                        Text(
                            text = s.scanQrConnect,
                            fontSize = 14.sp,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }

                SectionCard {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(
                            text = s.lanDiscovery,
                            color = Color.White,
                            fontSize = 15.sp,
                            fontWeight = FontWeight.Bold,
                            modifier = Modifier.weight(1f),
                        )
                        TextButton(onClick = onDiscoverHosts, enabled = !discoveringHosts) {
                            Text(
                                text = if (discoveringHosts) s.t("扫描中...", "Scanning...") else s.t("重新扫描", "Rescan"),
                                color = GpuColors.Accent,
                                fontSize = 12.sp,
                            )
                        }
                    }
                    if (discoveredHosts.isEmpty()) {
                        Text(
                            text = if (discoveringHosts) s.t(
                                "正在扫描局域网内的 GPUCap 主机 (UDP 8788)...",
                                "Scanning LAN for GPUCap hosts (UDP 8788)...",
                            ) else s.noLanHosts,
                            color = GpuColors.Dim,
                            fontSize = 12.sp,
                        )
                    } else {
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            discoveredHosts.forEach { host ->
                                Surface(
                                    onClick = { onSelectHistory(host.lanUrl) },
                                    shape = RoundedCornerShape(10.dp),
                                    color = GpuColors.InputBg,
                                    border = BorderStroke(1.dp, GpuColors.AccentBorder),
                                    modifier = Modifier.fillMaxWidth(),
                                ) {
                                    Row(
                                        verticalAlignment = Alignment.CenterVertically,
                                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                                    ) {
                                        Column(modifier = Modifier.weight(1f)) {
                                            Text(
                                                text = host.host.ifBlank { s.t("GPUCap 主机", "GPUCap Host") },
                                                color = Color.White,
                                                fontSize = 13.5.sp,
                                                fontWeight = FontWeight.Bold,
                                            )
                                            Text(
                                                text = host.lanUrl,
                                                color = GpuColors.Accent,
                                                fontSize = 11.5.sp,
                                                fontFamily = FontFamily.Monospace,
                                            )
                                        }
                                        Text(
                                            text = s.t("一键连接", "Connect"),
                                            color = GpuColors.EmeraldLight,
                                            fontSize = 12.sp,
                                            fontWeight = FontWeight.Bold,
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        val historyCard: @Composable () -> Unit = {
            SectionCard {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(
                        text = s.recentHosts,
                        color = Color.White,
                        fontSize = 15.sp,
                        fontWeight = FontWeight.Bold,
                        modifier = Modifier.weight(1f),
                    )
                    if (historyUrls.isNotEmpty()) {
                        OutlinedButton(
                            onClick = onPingAll,
                            shape = RoundedCornerShape(10.dp),
                            border = BorderStroke(1.dp, GpuColors.AccentBorder),
                            colors = ButtonDefaults.outlinedButtonColors(
                                containerColor = GpuColors.AccentBg,
                                contentColor = GpuColors.Accent,
                            ),
                            contentPadding = PaddingValues(horizontal = 10.dp, vertical = 6.dp),
                            modifier = Modifier.height(32.dp),
                        ) {
                            Icon(
                                imageVector = Icons.Rounded.NetworkCheck,
                                contentDescription = null,
                                modifier = Modifier.size(15.dp),
                            )
                            Spacer(modifier = Modifier.width(4.dp))
                            Text(text = s.pingAll, fontSize = 12.sp, fontWeight = FontWeight.Medium)
                        }
                    }
                }

                Spacer(modifier = Modifier.height(10.dp))

                if (historyUrls.isEmpty()) {
                    Text(
                        text = s.noRecentHosts,
                        color = GpuColors.Dim,
                        fontSize = 12.sp,
                    )
                } else {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        historyUrls.forEach { url ->
                            HistoryRow(
                                url = url,
                                pingStatus = pingMap[url] ?: PingStatus.Idle,
                                onSelect = { onSelectHistory(url) },
                                onRemove = { onRemoveHistory(url) },
                            )
                        }
                    }
                }
            }
        }

        Column(
            modifier = Modifier
                .widthIn(max = if (wide) 1040.dp else 560.dp)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 18.dp, vertical = 20.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.padding(bottom = 4.dp),
            ) {
                Surface(
                    shape = RoundedCornerShape(14.dp),
                    color = GpuColors.AccentBg,
                    border = BorderStroke(1.dp, GpuColors.Accent),
                    modifier = Modifier.size(46.dp),
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Icon(
                            imageVector = Icons.Rounded.Memory,
                            contentDescription = null,
                            tint = GpuColors.Accent,
                            modifier = Modifier.size(24.dp),
                        )
                    }
                }
                Spacer(modifier = Modifier.width(12.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = s.appTitle,
                        color = Color.White,
                        fontSize = 21.sp,
                        fontWeight = FontWeight.Bold,
                    )
                    Spacer(modifier = Modifier.height(2.dp))
                    Text(
                        text = s.heroSubtitle,
                        color = GpuColors.Dim,
                        fontSize = 12.sp,
                    )
                }
                Surface(
                    onClick = onToggleLang,
                    shape = RoundedCornerShape(9.dp),
                    color = GpuColors.AccentBg,
                    border = BorderStroke(1.dp, GpuColors.AccentBorder),
                ) {
                    Text(
                        text = if (isZh) "EN" else "中文",
                        color = GpuColors.Accent,
                        fontSize = 12.sp,
                        fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                    )
                }
            }

            if (wide) {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(14.dp),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Box(modifier = Modifier.weight(1f)) { connectAndQr() }
                    Box(modifier = Modifier.weight(1f)) { historyCard() }
                }
            } else {
                connectAndQr()
                historyCard()
            }
        }
    }

    if (dualChoice != null) {
        AlertDialog(
            onDismissRequest = onDismissDualChoice,
            containerColor = GpuColors.Card,
            titleContentColor = Color.White,
            textContentColor = GpuColors.Text,
            title = {
                Text(
                    text = s.t("选择连接方式", "Choose Connection"),
                    fontSize = 17.sp,
                    fontWeight = FontWeight.Bold,
                )
            },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(
                        text = s.t("检测到二维码同时包含以下两个地址：", "Both IPv4 and IPv6 addresses found in QR code:"),
                        color = GpuColors.Dim,
                        fontSize = 12.5.sp,
                    )
                    DualChoiceOption(
                        badge = s.t("局域网 IPv4（同 Wi-Fi 推荐）", "LAN IPv4 (Recommended on same Wi-Fi)"),
                        url = dualChoice.ipv4,
                        accent = GpuColors.Accent,
                        onClick = { onSelectHistory(dualChoice.ipv4) },
                    )
                    DualChoiceOption(
                        badge = s.t("外网 IPv6（跨网络访问）", "Public IPv6 (Cross-network)"),
                        url = dualChoice.ipv6,
                        accent = GpuColors.Emerald,
                        onClick = { onSelectHistory(dualChoice.ipv6) },
                    )
                }
            },
            confirmButton = {},
            dismissButton = {
                TextButton(onClick = onDismissDualChoice) {
                    Text(s.cancelBtn, color = GpuColors.Dim)
                }
            },
        )
    }
}

@Composable
private fun DualChoiceOption(
    badge: String,
    url: String,
    accent: Color,
    onClick: () -> Unit,
) {
    Surface(
        onClick = onClick,
        shape = RoundedCornerShape(12.dp),
        color = GpuColors.InputBg,
        border = BorderStroke(1.dp, accent.copy(alpha = 0.55f)),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp)) {
            Text(
                text = badge,
                color = accent,
                fontSize = 12.sp,
                fontWeight = FontWeight.SemiBold,
            )
            Spacer(modifier = Modifier.height(4.dp))
            Text(
                text = url,
                color = Color.White,
                fontSize = 12.5.sp,
                fontFamily = FontFamily.Monospace,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun HistoryRow(
    url: String,
    pingStatus: PingStatus,
    onSelect: () -> Unit,
    onRemove: () -> Unit,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Surface(
            onClick = onSelect,
            shape = RoundedCornerShape(10.dp),
            color = GpuColors.InputBg,
            border = BorderStroke(1.dp, GpuColors.Border),
            modifier = Modifier.weight(1f),
        ) {
            Text(
                text = url,
                color = GpuColors.Text,
                fontSize = 12.5.sp,
                fontFamily = FontFamily.Monospace,
                maxLines = 1,
                overflow = TextOverflow.MiddleEllipsis,
                modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
            )
        }

        PingBadge(status = pingStatus)

        Surface(
            onClick = onRemove,
            shape = RoundedCornerShape(10.dp),
            color = GpuColors.BadBg,
            border = BorderStroke(1.dp, GpuColors.BadBorder),
            modifier = Modifier.size(36.dp),
        ) {
            Box(contentAlignment = Alignment.Center) {
                Icon(
                    imageVector = Icons.Rounded.DeleteOutline,
                    contentDescription = "移除",
                    tint = GpuColors.BadLight,
                    modifier = Modifier.size(18.dp),
                )
            }
        }
    }
}

@Composable
private fun PingBadge(status: PingStatus) {
    val s = LocalStrings.current
    val (label, fg, bg, border) = when (status) {
        PingStatus.Idle -> Quad(s.t("待测", "Idle"), GpuColors.Dim, GpuColors.SurfaceBtn, GpuColors.Border)
        PingStatus.Checking -> Quad(s.t("测速...", "Ping..."), GpuColors.Accent, GpuColors.AccentBg, GpuColors.AccentBorder)
        is PingStatus.Online -> Quad("${status.latencyMs}ms", GpuColors.EmeraldLight, GpuColors.EmeraldBg, GpuColors.EmeraldBorder)
        PingStatus.Offline -> Quad(s.statusOffline, GpuColors.BadLight, GpuColors.BadBg, GpuColors.BadBorder)
    }
    val animBg by animateColorAsState(bg, label = "ping_bg")
    Surface(
        shape = RoundedCornerShape(10.dp),
        color = animBg,
        border = BorderStroke(1.dp, border),
    ) {
        Box(
            contentAlignment = Alignment.Center,
            modifier = Modifier.padding(horizontal = 9.dp, vertical = 9.dp),
        ) {
            Text(
                text = label,
                color = fg,
                fontSize = 11.sp,
                fontFamily = FontFamily.Monospace,
                maxLines = 1,
            )
        }
    }
}

private data class Quad(val a: String, val b: Color, val c: Color, val d: Color)

@Composable
fun SectionCard(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Card(
        shape = RoundedCornerShape(16.dp),
        colors = CardDefaults.cardColors(containerColor = GpuColors.Card),
        border = BorderStroke(1.dp, GpuColors.Border),
        elevation = CardDefaults.cardElevation(defaultElevation = 2.dp),
        modifier = modifier.fillMaxWidth(),
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            content()
        }
    }
}
