package com.gpucap.nativeapp

import android.graphics.Typeface
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.patrykandpatrick.vico.compose.cartesian.CartesianChartHost
import com.patrykandpatrick.vico.compose.cartesian.axis.rememberAxisGuidelineComponent
import com.patrykandpatrick.vico.compose.cartesian.axis.rememberAxisLabelComponent
import com.patrykandpatrick.vico.compose.cartesian.axis.rememberBottom
import com.patrykandpatrick.vico.compose.cartesian.axis.rememberStart
import com.patrykandpatrick.vico.compose.cartesian.layer.rememberLine
import com.patrykandpatrick.vico.compose.cartesian.layer.rememberLineCartesianLayer
import com.patrykandpatrick.vico.compose.cartesian.marker.rememberDefaultCartesianMarker
import com.patrykandpatrick.vico.compose.cartesian.rememberCartesianChart
import com.patrykandpatrick.vico.compose.cartesian.rememberVicoScrollState
import com.patrykandpatrick.vico.compose.common.component.rememberLineComponent
import com.patrykandpatrick.vico.compose.common.component.rememberShapeComponent
import com.patrykandpatrick.vico.compose.common.component.rememberTextComponent
import com.patrykandpatrick.vico.compose.common.fill
import com.patrykandpatrick.vico.compose.common.insets
import com.patrykandpatrick.vico.core.cartesian.axis.HorizontalAxis
import com.patrykandpatrick.vico.core.cartesian.axis.VerticalAxis
import com.patrykandpatrick.vico.core.cartesian.data.CartesianChartModelProducer
import com.patrykandpatrick.vico.core.cartesian.data.CartesianLayerRangeProvider
import com.patrykandpatrick.vico.core.cartesian.data.CartesianValueFormatter
import com.patrykandpatrick.vico.core.cartesian.data.lineSeries
import com.patrykandpatrick.vico.core.cartesian.layer.LineCartesianLayer
import com.patrykandpatrick.vico.core.cartesian.marker.DefaultCartesianMarker
import com.patrykandpatrick.vico.core.common.shader.ShaderProvider
import com.patrykandpatrick.vico.core.common.shape.CorneredShape
import java.util.Locale
import kotlin.math.roundToInt

data class TrendPoint(
    val ts: String,
    val value: Double,
)

data class TrendMetricSpec(
    val name: String,
    val unit: String,
    val decimals: Int,
    val color: Color,
    val points: List<TrendPoint>,
)

/**
 * 基于 Vico 图表库的原生趋势折线图卡片：
 * 支持平滑三次贝塞尔曲线、渐变填充、自适应 Y 轴与时间 X 轴，以及触摸任意采样点显示精确数值气泡。
 */
@Composable
fun NativeTrendCard(
    spec: TrendMetricSpec,
    compact: Boolean = false,
) {
    val pts = spec.points
    val latest = pts.lastOrNull()?.value
    val maxVal = pts.maxOfOrNull { it.value }
    val minVal = pts.minOfOrNull { it.value }

    fun fmt(v: Double?): String =
        if (v == null) "—" else String.format(Locale.US, "%.${spec.decimals}f", v)

    Surface(
        shape = RoundedCornerShape(if (compact) 11.dp else 14.dp),
        color = GpuColors.Card,
        border = BorderStroke(1.dp, GpuColors.Border),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(
            modifier = Modifier.padding(
                horizontal = if (compact) 10.dp else 12.dp,
                vertical = if (compact) 8.dp else 12.dp,
            ),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Box(
                    modifier = Modifier
                        .width(3.dp)
                        .height(if (compact) 12.dp else 14.dp)
                        .background(spec.color, RoundedCornerShape(2.dp)),
                )
                Spacer(modifier = Modifier.width(5.dp))
                Text(
                    text = spec.name,
                    color = Color.White,
                    fontSize = if (compact) 12.sp else 13.sp,
                    fontWeight = FontWeight.SemiBold,
                )
                if (!compact && spec.unit.isNotEmpty()) {
                    Spacer(modifier = Modifier.width(4.dp))
                    Text(
                        text = "(${spec.unit})",
                        color = GpuColors.Dim,
                        fontSize = 11.sp,
                    )
                }
                Spacer(modifier = Modifier.weight(1f))
                Row(
                    verticalAlignment = Alignment.Bottom,
                    horizontalArrangement = Arrangement.spacedBy(if (compact) 6.dp else 8.dp),
                ) {
                    Text(
                        text = if (latest != null) "${fmt(latest)} ${spec.unit}" else "—",
                        color = spec.color,
                        fontSize = if (compact) 12.sp else 13.sp,
                        fontWeight = FontWeight.Bold,
                        fontFamily = FontFamily.Monospace,
                    )
                    Text(
                        text = "高 ${fmt(maxVal)}",
                        color = GpuColors.Dim,
                        fontSize = if (compact) 10.sp else 10.5.sp,
                        fontFamily = FontFamily.Monospace,
                    )
                    Text(
                        text = "低 ${fmt(minVal)}",
                        color = GpuColors.Dim,
                        fontSize = if (compact) 10.sp else 10.5.sp,
                        fontFamily = FontFamily.Monospace,
                    )
                }
            }

            Spacer(modifier = Modifier.height(if (compact) 6.dp else 8.dp))

            val chartH = if (compact) 118.dp else 156.dp
            if (pts.size < 2) {
                Box(
                    contentAlignment = Alignment.Center,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(chartH)
                        .background(GpuColors.InputBg, RoundedCornerShape(8.dp)),
                ) {
                    Text(
                        text = if (pts.isEmpty()) "暂无采样数据" else "采样累积中...",
                        color = GpuColors.Dim,
                        fontSize = 12.sp,
                    )
                }
            } else {
                val ptsKey = remember(pts) { Triple(pts.size, pts.firstOrNull()?.ts ?: "", pts.lastOrNull()?.ts ?: "") }
                val modelProducer = remember { CartesianChartModelProducer() }
                LaunchedEffect(ptsKey) {
                    modelProducer.runTransaction {
                        lineSeries {
                            series(pts.map { it.value })
                        }
                    }
                }

                val rawMin = minVal ?: 0.0
                val rawMax = maxVal ?: 1.0
                val span = (rawMax - rawMin).let { if (it < 1e-4) 1.0 else it }
                val pad = span * 0.16
                val rangeProvider = remember(rawMin, rawMax, span) {
                    CartesianLayerRangeProvider.fixed(
                        minY = (rawMin - pad).let { if (rawMin >= 0.0 && it < 0.0) 0.0 else it },
                        maxY = rawMax + pad,
                    )
                }

                val yFormatter = remember(spec.decimals) {
                    CartesianValueFormatter { _, value, _ ->
                        String.format(Locale.US, "%.${spec.decimals}f", value)
                    }
                }
                val xFormatter = remember(pts) {
                    CartesianValueFormatter { _, value, _ ->
                        val idx = value.roundToInt().coerceIn(0, (pts.size - 1).coerceAtLeast(0))
                        pts.getOrNull(idx)?.ts ?: ""
                    }
                }

                val axisLabel = rememberAxisLabelComponent(
                    color = GpuColors.Dim,
                    textSize = if (compact) 9.sp else 10.sp,
                    typeface = Typeface.MONOSPACE,
                )
                val gridLine = rememberAxisGuidelineComponent(
                    fill = fill(Color.White.copy(alpha = 0.07f)),
                )

                val markerLabel = rememberTextComponent(
                    color = Color.White,
                    textSize = 10.5.sp,
                    typeface = Typeface.MONOSPACE,
                    padding = insets(horizontal = 7.dp, vertical = 3.dp),
                    background = rememberShapeComponent(
                        fill = fill(Color(0xEE182436)),
                        shape = CorneredShape.rounded(allPercent = 25),
                        strokeFill = fill(spec.color.copy(alpha = 0.7f)),
                        strokeThickness = 1.dp,
                    ),
                )
                val markerGuideline = rememberLineComponent(
                    fill = fill(spec.color.copy(alpha = 0.55f)),
                    thickness = 1.dp,
                )
                val markerFormatter = remember(pts, spec.decimals, spec.unit) {
                    DefaultCartesianMarker.ValueFormatter { _, targets ->
                        val t = targets.firstOrNull()
                        val idx = t?.x?.roundToInt()?.coerceIn(0, (pts.size - 1).coerceAtLeast(0)) ?: 0
                        val ts = pts.getOrNull(idx)?.ts ?: ""
                        val yVal = pts.getOrNull(idx)?.value
                        if (ts.isNotEmpty() && yVal != null) {
                            "$ts  ${String.format(Locale.US, "%.${spec.decimals}f", yVal)} ${spec.unit}".trim()
                        } else {
                            ""
                        }
                    }
                }
                val marker = rememberDefaultCartesianMarker(
                    label = markerLabel,
                    valueFormatter = markerFormatter,
                    guideline = markerGuideline,
                )

                val areaShader = remember(spec.color) {
                    ShaderProvider.verticalGradient(
                        spec.color.copy(alpha = 0.34f).toArgb(),
                        Color.Transparent.toArgb(),
                    )
                }

                val chart = rememberCartesianChart(
                    rememberLineCartesianLayer(
                        lineProvider = LineCartesianLayer.LineProvider.series(
                            LineCartesianLayer.rememberLine(
                                fill = LineCartesianLayer.LineFill.single(fill(spec.color)),
                                areaFill = LineCartesianLayer.AreaFill.single(fill(areaShader)),
                                pointConnector = remember {
                                    LineCartesianLayer.PointConnector.cubic(curvature = 0.35f)
                                },
                            ),
                        ),
                        rangeProvider = rangeProvider,
                    ),
                    startAxis = VerticalAxis.rememberStart(
                        label = axisLabel,
                        line = null,
                        tick = null,
                        guideline = gridLine,
                        valueFormatter = yFormatter,
                        itemPlacer = VerticalAxis.ItemPlacer.count({ if (compact) 4 else 5 }),
                    ),
                    bottomAxis = HorizontalAxis.rememberBottom(
                        label = axisLabel,
                        line = null,
                        tick = null,
                        guideline = null,
                        valueFormatter = xFormatter,
                        itemPlacer = remember(ptsKey) {
                            HorizontalAxis.ItemPlacer.aligned(
                                spacing = { (pts.size / 2).coerceAtLeast(1) },
                                addExtremeLabelPadding = true,
                            )
                        },
                    ),
                    marker = marker,
                )

                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(chartH)
                        .background(GpuColors.InputBg, RoundedCornerShape(8.dp))
                        .padding(start = 2.dp, end = 6.dp, top = 6.dp, bottom = 2.dp),
                ) {
                    CartesianChartHost(
                        chart = chart,
                        modelProducer = modelProducer,
                        scrollState = rememberVicoScrollState(scrollEnabled = false),
                        animationSpec = null,
                        modifier = Modifier.fillMaxWidth().height(chartH),
                    )
                }
            }
        }
    }
}
