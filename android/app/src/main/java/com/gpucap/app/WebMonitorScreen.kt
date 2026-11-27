package com.gpucap.app

import android.annotation.SuppressLint
import android.app.DownloadManager
import android.content.Context
import android.net.Uri
import android.os.Environment
import android.view.ViewGroup
import android.webkit.URLUtil
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.SwapHoriz
import androidx.compose.material.icons.rounded.WifiOff
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView

enum class ConnStatus {
    Connecting, Connected, Failed
}

/**
 * 内嵌网页监控页：
 * 顶部状态胶囊栏 + 悬浮加载进度条 + Material 3 下拉刷新 WebView + 断线重连卡片。
 */
@OptIn(ExperimentalMaterial3Api::class)
@SuppressLint("SetJavaScriptEnabled")
@Composable
fun WebMonitorScreen(
    url: String,
    isZh: Boolean = false,
    onToggleLang: () -> Unit = {},
    onSwitchHost: () -> Unit,
    onToast: (String) -> Unit,
    onCanGoBackChanged: (() -> Boolean) -> Unit,
) {
    fun t(zh: String, en: String): String = if (isZh) zh else en
    val context = LocalContext.current
    var connStatus by remember(url) { mutableStateOf(ConnStatus.Connecting) }
    var progress by remember(url) { mutableIntStateOf(0) }
    var isRefreshing by remember { mutableStateOf(false) }
    var errorMessage by remember(url) { mutableStateOf<String?>(null) }

    val webView = remember {
        WebView(context).apply {
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT,
            )
            setBackgroundColor(android.graphics.Color.parseColor("#0B0F17"))
            settings.apply {
                javaScriptEnabled = true
                domStorageEnabled = true
                loadWithOverviewMode = true
                useWideViewPort = true
                builtInZoomControls = true
                displayZoomControls = false
                cacheMode = WebSettings.LOAD_NO_CACHE
            }
            setDownloadListener { dlUrl, _, contentDisposition, mimeType, _ ->
                try {
                    val req = DownloadManager.Request(Uri.parse(dlUrl))
                    val fileName = URLUtil.guessFileName(dlUrl, contentDisposition, mimeType)
                    req.setTitle(fileName)
                    req.setDescription("GPUCap 会话取证导出")
                    req.setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
                    req.setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, fileName)
                    val dm = context.getSystemService(Context.DOWNLOAD_SERVICE) as? DownloadManager
                    if (dm != null) {
                        dm.enqueue(req)
                        onToast(t("开始下载：$fileName", "Downloading: $fileName"))
                    }
                } catch (e: Exception) {
                    onToast(t("下载失败：${e.message}", "Download failed: ${e.message}"))
                }
            }
        }
    }

    DisposableEffect(webView) {
        onCanGoBackChanged {
            if (webView.canGoBack()) {
                webView.goBack()
                true
            } else {
                false
            }
        }
        onDispose {
            webView.destroy()
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(GpuColors.TopBar)
            .statusBarsPadding()
            .navigationBarsPadding(),
    ) {
        // 顶部稳定胶囊控制栏（避免与 WebView 硬件层产生离屏模糊闪烁）
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            modifier = Modifier
                .fillMaxWidth()
                .background(GpuColors.TopBar)
                .padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            Surface(
                shape = RoundedCornerShape(99.dp),
                color = Color(0xFF162032),
                border = BorderStroke(1.dp, GpuColors.Border),
                modifier = Modifier.weight(1f),
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 7.dp),
                ) {
                    val dotColor = when (connStatus) {
                        ConnStatus.Connecting -> GpuColors.Accent
                        ConnStatus.Connected -> GpuColors.Emerald
                        ConnStatus.Failed -> GpuColors.Bad
                    }
                    Box(
                        modifier = Modifier
                            .size(7.dp)
                            .clip(CircleShape)
                            .background(dotColor),
                    )
                    Spacer(modifier = Modifier.width(7.dp))
                    Text(
                        text = url,
                        color = GpuColors.Text,
                        fontSize = 12.sp,
                        fontFamily = FontFamily.Monospace,
                        maxLines = 1,
                        overflow = TextOverflow.MiddleEllipsis,
                    )
                }
            }

            Surface(
                onClick = {
                    val nextZh = !isZh
                    onToggleLang()
                    val code = if (nextZh) "zh-CN" else "en"
                    webView.evaluateJavascript("localStorage.setItem('gpucap_lang', '$code');", null)
                    webView.reload()
                },
                shape = RoundedCornerShape(10.dp),
                color = GpuColors.SurfaceBtn,
            ) {
                Text(
                    text = if (isZh) "EN" else "中文",
                    color = GpuColors.Accent,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 7.dp),
                )
            }

            Surface(
                onClick = {
                    errorMessage = null
                    connStatus = ConnStatus.Connecting
                    webView.reload()
                },
                shape = RoundedCornerShape(10.dp),
                color = GpuColors.SurfaceBtn,
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 7.dp),
                ) {
                    Icon(
                        imageVector = Icons.Rounded.Refresh,
                        contentDescription = t("刷新", "Reload"),
                        tint = GpuColors.Text,
                        modifier = Modifier.size(15.dp),
                    )
                    Spacer(modifier = Modifier.width(4.dp))
                    Text(text = t("刷新", "Reload"), color = GpuColors.Text, fontSize = 12.sp)
                }
            }

            Surface(
                onClick = onSwitchHost,
                shape = RoundedCornerShape(10.dp),
                color = GpuColors.AccentBg,
                border = BorderStroke(1.dp, GpuColors.AccentBorder),
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 7.dp),
                ) {
                    Icon(
                        imageVector = Icons.Rounded.SwapHoriz,
                        contentDescription = t("切换主机", "Switch Host"),
                        tint = GpuColors.Accent,
                        modifier = Modifier.size(15.dp),
                    )
                    Spacer(modifier = Modifier.width(4.dp))
                    Text(text = t("切换", "Switch"), color = GpuColors.Accent, fontSize = 12.sp)
                }
            }
        }

        // WebView 下拉刷新容器 + 悬浮加载进度条 + 断线提示卡片
        Box(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .background(GpuColors.Bg),
        ) {
            PullToRefreshBox(
                isRefreshing = isRefreshing,
                onRefresh = {
                    isRefreshing = true
                    errorMessage = null
                    connStatus = ConnStatus.Connecting
                    webView.reload()
                },
                modifier = Modifier.fillMaxSize(),
            ) {
                AndroidView(
                    factory = {
                        webView.apply {
                            webChromeClient = object : WebChromeClient() {
                                override fun onProgressChanged(view: WebView?, newProgress: Int) {
                                    progress = newProgress
                                    if (newProgress >= 100) {
                                        isRefreshing = false
                                    }
                                }
                            }
                            webViewClient = object : WebViewClient() {
                                override fun onPageFinished(view: WebView?, loadedUrl: String?) {
                                    super.onPageFinished(view, loadedUrl)
                                    val code = if (isZh) "zh-CN" else "en"
                                    view?.evaluateJavascript(
                                        "if(!localStorage.getItem('gpucap_lang')){localStorage.setItem('gpucap_lang','$code');}",
                                        null,
                                    )
                                    isRefreshing = false
                                    progress = 100
                                    if (errorMessage == null) {
                                        connStatus = ConnStatus.Connected
                                    }
                                }

                                override fun onReceivedError(
                                    view: WebView?,
                                    request: WebResourceRequest?,
                                    error: WebResourceError?,
                                ) {
                                    if (request != null && request.isForMainFrame) {
                                        isRefreshing = false
                                        progress = 100
                                        connStatus = ConnStatus.Failed
                                        errorMessage = t(
                                            "无法连接到 $url\n请检查电脑端是否已开启「网页服务」，且网络链路互通。",
                                            "Could not connect to $url.\nMake sure the Windows host web service is running and reachable.",
                                        )
                                    }
                                }
                            }
                            loadUrl(url)
                        }
                    },
                    update = { view ->
                        if (view.tag != url) {
                            view.tag = url
                            view.loadUrl(url)
                        }
                    },
                    modifier = Modifier.fillMaxSize(),
                )
            }

            if (progress in 1..99) {
                LinearProgressIndicator(
                    progress = { progress / 100f },
                    color = GpuColors.Accent,
                    trackColor = Color.Transparent,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(2.dp)
                        .align(Alignment.TopCenter),
                )
            }

            if (errorMessage != null) {
                Box(
                    contentAlignment = Alignment.Center,
                    modifier = Modifier
                        .fillMaxSize()
                        .background(GpuColors.Bg)
                        .padding(24.dp),
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
                                text = t("无法连接到 GPUCap 服务", "Unable to Connect to GPUCap"),
                                color = GpuColors.BadLight,
                                fontSize = 17.sp,
                                fontWeight = FontWeight.Bold,
                            )
                            Spacer(modifier = Modifier.height(8.dp))
                            Text(
                                text = errorMessage ?: "",
                                color = GpuColors.Dim,
                                fontSize = 13.sp,
                                textAlign = TextAlign.Center,
                                lineHeight = 19.sp,
                            )
                            Spacer(modifier = Modifier.height(18.dp))
                            Button(
                                onClick = {
                                    errorMessage = null
                                    connStatus = ConnStatus.Connecting
                                    webView.loadUrl(url)
                                },
                                shape = RoundedCornerShape(12.dp),
                                colors = ButtonDefaults.buttonColors(
                                    containerColor = GpuColors.Emerald,
                                    contentColor = GpuColors.EmeraldText,
                                ),
                                contentPadding = PaddingValues(vertical = 12.dp),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Text(t("重新连接", "Reconnect"), fontSize = 14.sp, fontWeight = FontWeight.Bold)
                            }
                            Spacer(modifier = Modifier.height(10.dp))
                            OutlinedButton(
                                onClick = onSwitchHost,
                                shape = RoundedCornerShape(12.dp),
                                border = BorderStroke(1.dp, GpuColors.Accent),
                                colors = ButtonDefaults.outlinedButtonColors(contentColor = GpuColors.Accent),
                                contentPadding = PaddingValues(vertical = 12.dp),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Text(t("返回选择或扫码连接", "Back to Host Selection"), fontSize = 14.sp, fontWeight = FontWeight.Bold)
                            }
                        }
                    }
                }
            }
        }
    }
}
