package com.gpucap.nativeapp

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.OptIn
import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.ExperimentalGetImage
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.FlashlightOff
import androidx.compose.material.icons.rounded.FlashlightOn
import androidx.compose.material.icons.rounded.PhotoLibrary
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.google.mlkit.vision.barcode.BarcodeScannerOptions
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

@OptIn(ExperimentalGetImage::class)
@Composable
fun QrScannerScreen(
    onQrDetected: (String) -> Unit,
    onToast: (String) -> Unit,
    onClose: () -> Unit,
) {
    val s = LocalStrings.current
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val delivered = remember { AtomicBoolean(false) }

    var cameraRef by remember { mutableStateOf<Camera?>(null) }
    var hasFlash by remember { mutableStateOf(false) }
    var torchOn by remember { mutableStateOf(false) }

    val scanner = remember {
        val opts = BarcodeScannerOptions.Builder()
            .setBarcodeFormats(Barcode.FORMAT_QR_CODE)
            .build()
        BarcodeScanning.getClient(opts)
    }

    val albumLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.GetContent(),
    ) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        try {
            val image = InputImage.fromFilePath(context, uri)
            scanner.process(image)
                .addOnSuccessListener { barcodes ->
                    val hit = barcodes.firstNotNullOfOrNull { it.rawValue?.trim()?.takeIf { s -> s.isNotEmpty() } }
                    if (hit != null && delivered.compareAndSet(false, true)) {
                        onQrDetected(hit)
                    } else if (hit == null) {
                        onToast(s.t("未在所选图片中识别到有效二维码", "No valid QR code found in selected image"))
                    }
                }
                .addOnFailureListener {
                    onToast(s.t("图片二维码识别失败", "Failed to scan QR code from image"))
                }
        } catch (_: Exception) {
            onToast(s.t("无法读取所选图片", "Could not read selected image"))
        }
    }

    val analyzerExecutor = remember { Executors.newSingleThreadExecutor() }

    DisposableEffect(Unit) {
        onDispose {
            runCatching { cameraRef?.cameraControl?.enableTorch(false) }
            analyzerExecutor.shutdown()
            scanner.close()
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black),
    ) {
        AndroidView(
            factory = { ctx ->
                val previewView = PreviewView(ctx).apply {
                    scaleType = PreviewView.ScaleType.FILL_CENTER
                }
                val providerFuture = ProcessCameraProvider.getInstance(ctx)
                providerFuture.addListener({
                    val provider = providerFuture.get()
                    val preview = Preview.Builder().build().apply {
                        surfaceProvider = previewView.surfaceProvider
                    }
                    val analysis = ImageAnalysis.Builder()
                        .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                        .build()
                        .apply {
                            setAnalyzer(analyzerExecutor) { proxy ->
                                val mediaImage = proxy.image
                                if (mediaImage == null || delivered.get()) {
                                    proxy.close()
                                    return@setAnalyzer
                                }
                                val input = InputImage.fromMediaImage(
                                    mediaImage,
                                    proxy.imageInfo.rotationDegrees,
                                )
                                scanner.process(input)
                                    .addOnSuccessListener { barcodes ->
                                        val hit = barcodes.firstNotNullOfOrNull {
                                            it.rawValue?.trim()?.takeIf { s -> s.isNotEmpty() }
                                        }
                                        if (hit != null && delivered.compareAndSet(false, true)) {
                                            onQrDetected(hit)
                                        }
                                    }
                                    .addOnCompleteListener {
                                        proxy.close()
                                    }
                            }
                        }

                    runCatching {
                        provider.unbindAll()
                        val cam = provider.bindToLifecycle(
                            lifecycleOwner,
                            CameraSelector.DEFAULT_BACK_CAMERA,
                            preview,
                            analysis,
                        )
                        cameraRef = cam
                        hasFlash = cam.cameraInfo.hasFlashUnit()
                    }
                }, ContextCompat.getMainExecutor(ctx))
                previewView
            },
            modifier = Modifier.fillMaxSize(),
        )

        val inf = rememberInfiniteTransition(label = "scan_line")
        val lineProgress by inf.animateFloat(
            initialValue = 0f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 2000, easing = LinearEasing),
                repeatMode = RepeatMode.Restart,
            ),
            label = "scan_progress",
        )

        Canvas(
            modifier = Modifier
                .fillMaxSize()
                .graphicsLayer(compositingStrategy = CompositingStrategy.Offscreen),
        ) {
            val boxSize = minOf(size.width, size.height) * 0.64f
            val left = (size.width - boxSize) / 2f
            val top = (size.height - boxSize) * 0.44f
            val r = 24f

            drawRect(color = Color(0x99000000))
            drawRoundRect(
                color = Color.Transparent,
                topLeft = Offset(left, top),
                size = Size(boxSize, boxSize),
                cornerRadius = CornerRadius(r, r),
                blendMode = BlendMode.Clear,
            )
            drawRoundRect(
                color = Color(0x44FFFFFF),
                topLeft = Offset(left, top),
                size = Size(boxSize, boxSize),
                cornerRadius = CornerRadius(r, r),
                style = Stroke(width = 2f),
            )

            val cLen = boxSize * 0.11f
            val stroke = 8f
            val right = left + boxSize
            val bottom = top + boxSize
            drawLine(GpuColors.Accent, Offset(left, top + cLen), Offset(left, top), stroke, StrokeCap.Round)
            drawLine(GpuColors.Accent, Offset(left, top), Offset(left + cLen, top), stroke, StrokeCap.Round)

            drawLine(GpuColors.Accent, Offset(right - cLen, top), Offset(right, top), stroke, StrokeCap.Round)
            drawLine(GpuColors.Accent, Offset(right, top), Offset(right, top + cLen), stroke, StrokeCap.Round)

            drawLine(GpuColors.Accent, Offset(left, bottom - cLen), Offset(left, bottom), stroke, StrokeCap.Round)
            drawLine(GpuColors.Accent, Offset(left, bottom), Offset(left + cLen, bottom), stroke, StrokeCap.Round)

            drawLine(GpuColors.Accent, Offset(right - cLen, bottom), Offset(right, bottom), stroke, StrokeCap.Round)
            drawLine(GpuColors.Accent, Offset(right, bottom), Offset(right, bottom - cLen), stroke, StrokeCap.Round)

            val y = top + 16f + (boxSize - 32f) * lineProgress
            drawLine(
                brush = Brush.horizontalGradient(
                    colors = listOf(Color.Transparent, GpuColors.Accent, Color.Transparent),
                    startX = left + 12f,
                    endX = right - 12f,
                ),
                start = Offset(left + 12f, y),
                end = Offset(right - 12f, y),
                strokeWidth = 5f,
            )
        }

        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier
                .fillMaxWidth()
                .statusBarsPadding()
                .padding(horizontal = 14.dp, vertical = 10.dp)
                .align(Alignment.TopCenter),
        ) {
            Surface(
                onClick = onClose,
                shape = RoundedCornerShape(99.dp),
                color = Color(0x880F141E),
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
                ) {
                    Icon(
                        imageVector = Icons.Rounded.Close,
                        contentDescription = s.closeBtn,
                        tint = GpuColors.Text,
                        modifier = Modifier.size(16.dp),
                    )
                    Spacer(modifier = Modifier.width(4.dp))
                    Text(text = s.closeBtn, color = GpuColors.Text, fontSize = 13.sp)
                }
            }

            Box(
                contentAlignment = Alignment.Center,
                modifier = Modifier.weight(1f),
            ) {
                Text(
                    text = s.t("扫描连接二维码", "Scan QR Code"),
                    color = Color.White,
                    fontSize = 16.sp,
                    fontWeight = FontWeight.Bold,
                )
            }

            Surface(
                onClick = { albumLauncher.launch("image/*") },
                shape = RoundedCornerShape(99.dp),
                color = Color(0x8816263A),
                border = BorderStroke(1.dp, GpuColors.Accent.copy(alpha = 0.6f)),
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
                ) {
                    Icon(
                        imageVector = Icons.Rounded.PhotoLibrary,
                        contentDescription = s.t("相册", "Gallery"),
                        tint = GpuColors.Accent,
                        modifier = Modifier.size(16.dp),
                    )
                    Spacer(modifier = Modifier.width(4.dp))
                    Text(text = s.t("相册", "Gallery"), color = GpuColors.Accent, fontSize = 13.sp)
                }
            }
        }

        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(16.dp),
            modifier = Modifier
                .fillMaxWidth()
                .align(Alignment.BottomCenter)
                .navigationBarsPadding()
                .padding(horizontal = 24.dp, vertical = 34.dp),
        ) {
            Text(
                text = s.t("对准 GPUCap 二维码，识别后将自动连接", "Point at GPUCap QR code to connect automatically"),
                color = GpuColors.Text,
                fontSize = 13.sp,
            )

            if (hasFlash) {
                OutlinedButton(
                    onClick = {
                        val next = !torchOn
                        torchOn = next
                        cameraRef?.cameraControl?.enableTorch(next)
                    },
                    shape = RoundedCornerShape(99.dp),
                    border = BorderStroke(
                        1.dp,
                        if (torchOn) GpuColors.Accent else Color(0x44FFFFFF),
                    ),
                    colors = ButtonDefaults.outlinedButtonColors(
                        containerColor = Color(0x99121A28),
                        contentColor = if (torchOn) GpuColors.Accent else Color.White,
                    ),
                    contentPadding = PaddingValues(horizontal = 18.dp, vertical = 10.dp),
                ) {
                    Icon(
                        imageVector = if (torchOn) Icons.Rounded.FlashlightOff else Icons.Rounded.FlashlightOn,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp),
                    )
                    Spacer(modifier = Modifier.width(6.dp))
                    Text(
                        text = if (torchOn) s.t("关闭手电筒", "Torch Off") else s.t("打开手电筒", "Torch On"),
                        fontSize = 13.sp,
                    )
                }
            }
        }
    }
}
