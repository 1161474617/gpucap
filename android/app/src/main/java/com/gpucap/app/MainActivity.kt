package com.gpucap.app

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Snackbar
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch

/**
 * GPUCap 安卓客户端主入口：
 * 采用 ViewModel + StateFlow 单向数据流、DataStore 持久化与 Edge-to-Edge。
 */
class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
        )

        setContent {
            GpuCapTheme {
                val vm: MainViewModel = viewModel()
                val uiState by vm.uiState.collectAsStateWithLifecycle()
                val scope = rememberCoroutineScope()
                val snackbarHostState = remember { SnackbarHostState() }

                var lastBackAt by remember { mutableLongStateOf(0L) }
                var webBackHandler by remember { mutableStateOf<() -> Boolean>({ false }) }

                LaunchedEffect(Unit) {
                    vm.toastFlow.collect { msg ->
                        snackbarHostState.currentSnackbarData?.dismiss()
                        snackbarHostState.showSnackbar(msg, duration = SnackbarDuration.Short)
                    }
                }

                val cameraPermLauncher = rememberLauncherForActivityResult(
                    contract = ActivityResultContracts.RequestPermission(),
                ) { granted ->
                    if (granted) {
                        vm.openQrScanner()
                    } else {
                        scope.launch {
                            val res = snackbarHostState.showSnackbar(
                                message = if (uiState.isZh) "需要相机权限才能扫描二维码" else "Camera permission is required to scan QR codes",
                                actionLabel = if (uiState.isZh) "去设置" else "Settings",
                                duration = SnackbarDuration.Long,
                            )
                            if (res == SnackbarResult.ActionPerformed) {
                                startActivity(
                                    Intent(
                                        Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                                        Uri.fromParts("package", packageName, null),
                                    ),
                                )
                            }
                        }
                    }
                }

                fun startQrScan() {
                    val granted = ContextCompat.checkSelfPermission(
                        this@MainActivity,
                        Manifest.permission.CAMERA,
                    ) == PackageManager.PERMISSION_GRANTED
                    if (granted) {
                        vm.openQrScanner()
                    } else {
                        cameraPermLauncher.launch(Manifest.permission.CAMERA)
                    }
                }

                BackHandler(enabled = uiState.page != AppPage.Connect || uiState.currentUrl.isNotEmpty()) {
                    when (uiState.page) {
                        AppPage.QrScan -> vm.closeQrScanner()
                        AppPage.Web -> {
                            if (!webBackHandler()) {
                                val now = System.currentTimeMillis()
                                if (now - lastBackAt < 2000L) {
                                    finish()
                                } else {
                                    lastBackAt = now
                                    vm.emitToast(if (uiState.isZh) "再按一次返回键退出，或点击右上角「切换」" else "Press back again to exit, or tap Switch")
                                }
                            }
                        }
                        AppPage.Connect -> finish()
                    }
                }

                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .background(GpuColors.Bg),
                ) {
                    if (uiState.ready) {
                        AnimatedContent(
                            targetState = uiState.page,
                            transitionSpec = { fadeIn() togetherWith fadeOut() },
                            label = "page_transition",
                        ) { target ->
                            when (target) {
                                AppPage.Connect -> ConnectScreen(
                                    inputUrl = uiState.inputUrl,
                                    onInputChange = vm::onInputChange,
                                    historyUrls = uiState.historyUrls,
                                    pingMap = uiState.pingMap,
                                    dualChoice = uiState.dualChoice,
                                    onDismissDualChoice = vm::dismissDualChoice,
                                    onConnectManual = vm::connectManual,
                                    onStartQrScan = ::startQrScan,
                                    onSelectHistory = vm::openAndSave,
                                    onRemoveHistory = vm::removeHistory,
                                    onPingAll = vm::pingAll,
                                    isZh = uiState.isZh,
                                    onToggleLang = { vm.setAppLang(!uiState.isZh) },
                                )

                                AppPage.Web -> WebMonitorScreen(
                                    url = uiState.currentUrl,
                                    isZh = uiState.isZh,
                                    onToggleLang = { vm.setAppLang(!uiState.isZh) },
                                    onSwitchHost = vm::switchToConnect,
                                    onToast = vm::emitToast,
                                    onCanGoBackChanged = { webBackHandler = it },
                                )

                                AppPage.QrScan -> QrScannerScreen(
                                    isZh = uiState.isZh,
                                    onQrDetected = vm::handleQrDetected,
                                    onToast = vm::emitToast,
                                    onClose = vm::closeQrScanner,
                                )
                            }
                        }
                    }

                    if (uiState.ready && uiState.showLangPicker) {
                        AlertDialog(
                            onDismissRequest = {},
                            containerColor = GpuColors.Card,
                            title = {
                                Text(
                                    text = "Choose Language / 请选择语言",
                                    color = androidx.compose.ui.graphics.Color.White,
                                    fontWeight = FontWeight.Bold,
                                    fontSize = 17.sp,
                                )
                            },
                            text = {
                                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                    Text(
                                        text = "Select your preferred display language. You can change this anytime.\n请选择显示语言，之后可随时在界面切换。",
                                        color = GpuColors.Dim,
                                        fontSize = 13.sp,
                                    )
                                    Row(
                                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                                        modifier = Modifier.fillMaxWidth(),
                                    ) {
                                        Button(
                                            onClick = { vm.setAppLang(false) },
                                            shape = RoundedCornerShape(10.dp),
                                            colors = ButtonDefaults.buttonColors(
                                                containerColor = GpuColors.Accent,
                                                contentColor = GpuColors.AccentDark,
                                            ),
                                            contentPadding = PaddingValues(vertical = 11.dp),
                                            modifier = Modifier.weight(1f),
                                        ) {
                                            Text("English", fontWeight = FontWeight.Bold)
                                        }
                                        Button(
                                            onClick = { vm.setAppLang(true) },
                                            shape = RoundedCornerShape(10.dp),
                                            colors = ButtonDefaults.buttonColors(
                                                containerColor = GpuColors.Emerald,
                                                contentColor = GpuColors.EmeraldText,
                                            ),
                                            contentPadding = PaddingValues(vertical = 11.dp),
                                            modifier = Modifier.weight(1f),
                                        ) {
                                            Text("简体中文", fontWeight = FontWeight.Bold)
                                        }
                                    }
                                }
                            },
                            confirmButton = {},
                        )
                    }

                    SnackbarHost(
                        hostState = snackbarHostState,
                        modifier = Modifier
                            .align(Alignment.BottomCenter)
                            .navigationBarsPadding(),
                    ) { data ->
                        Snackbar(
                            containerColor = GpuColors.SurfaceBtn,
                            contentColor = GpuColors.Text,
                            action = {
                                data.visuals.actionLabel?.let { label ->
                                    TextButton(onClick = { data.performAction() }) {
                                        Text(label, color = GpuColors.Accent)
                                    }
                                }
                            },
                        ) {
                            Text(data.visuals.message)
                        }
                    }
                }
            }
        }
    }
}
