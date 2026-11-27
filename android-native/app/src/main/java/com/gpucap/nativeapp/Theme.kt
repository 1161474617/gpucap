package com.gpucap.nativeapp

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

object GpuColors {
    val Bg = Color(0xFF0B0F17)
    val TopBar = Color(0xFF0F141E)
    val Card = Color(0xFF121A28)
    val InputBg = Color(0xFF0D131F)
    val HeadBg = Color(0xFF171F2E)
    val Border = Color(0xFF233044)
    val Text = Color(0xFFE2E8F0)
    val Dim = Color(0xFF8492A6)
    val Accent = Color(0xFF38BDF8)
    val AccentDark = Color(0xFF071926)
    val AccentBg = Color(0xFF16263A)
    val AccentBorder = Color(0xFF264363)
    val Emerald = Color(0xFF10B981)
    val EmeraldText = Color(0xFF06281E)
    val EmeraldBg = Color(0xFF0F2D24)
    val EmeraldBorder = Color(0xFF1D5B47)
    val EmeraldLight = Color(0xFF6EE7B7)
    val SurfaceBtn = Color(0xFF1E293B)
    val Bad = Color(0xFFEF4444)
    val BadBg = Color(0xFF2A161B)
    val BadBorder = Color(0xFF5B212A)
    val BadLight = Color(0xFFFCA5A5)
    val Warn = Color(0xFFF59E0B)
    val WarnBg = Color(0xFF2B2110)
    val WarnBorder = Color(0xFF6B4C16)
    val WarnLight = Color(0xFFFCD34D)
    val Rose = Color(0xFFFB7185)
    val Crimson = Color(0xFFF43F5E)
    val Orange = Color(0xFFFB923C)
    val Violet = Color(0xFFA78BFA)
    val Teal = Color(0xFF2DD4BF)
    val Pink = Color(0xFFF472B6)
}

private val DarkScheme = darkColorScheme(
    primary = GpuColors.Accent,
    onPrimary = GpuColors.AccentDark,
    secondary = GpuColors.Emerald,
    onSecondary = GpuColors.EmeraldText,
    background = GpuColors.Bg,
    onBackground = GpuColors.Text,
    surface = GpuColors.Card,
    onSurface = GpuColors.Text,
    surfaceVariant = GpuColors.SurfaceBtn,
    onSurfaceVariant = GpuColors.Dim,
    outline = GpuColors.Border,
    error = GpuColors.Bad,
    onError = Color.White,
)

@Composable
fun GpuCapTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = DarkScheme,
        content = content,
    )
}
