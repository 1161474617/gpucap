package com.gpucap.nativeapp

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
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
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
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch

/**
 * GPUCap 纯原生安卓客户端主入口（无 WebView）。
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

                LaunchedEffect(uiState.keepScreenOn) {
                    if (uiState.keepScreenOn) {
                        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                    } else {
                        window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                    }
                }

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
                                message = "需要相机权限才能扫描二维码",
                                actionLabel = "去设置",
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
                        AppPage.GpuTable -> vm.closeGpuTable()
                        AppPage.Monitor -> {
                            val now = System.currentTimeMillis()
                            if (now - lastBackAt < 2000L) {
                                finish()
                            } else {
                                lastBackAt = now
                                vm.emitToast("再按一次返回键退出，或点击右上角「切换」")
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
                                    discoveredHosts = uiState.discoveredHosts,
                                    discoveringHosts = uiState.discoveringHosts,
                                    dualChoice = uiState.dualChoice,
                                    onDismissDualChoice = vm::dismissDualChoice,
                                    onConnectManual = vm::connectManual,
                                    onStartQrScan = ::startQrScan,
                                    onSelectHistory = vm::openAndSave,
                                    onRemoveHistory = vm::removeHistory,
                                    onPingAll = vm::pingAll,
                                    onDiscoverHosts = vm::discoverLanHosts,
                                )

                                AppPage.Monitor -> NativeMonitorScreen(
                                    uiState = uiState,
                                    vm = vm,
                                )

                                AppPage.QrScan -> QrScannerScreen(
                                    onQrDetected = vm::handleQrDetected,
                                    onToast = vm::emitToast,
                                    onClose = vm::closeQrScanner,
                                )

                                AppPage.GpuTable -> GpuTableScreen(
                                    uiState = uiState,
                                    vm = vm,
                                )
                            }
                        }
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
