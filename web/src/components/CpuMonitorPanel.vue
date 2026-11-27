<script setup>
import { computed, ref } from 'vue'
import { t, tl } from '../i18n'

const props = defineProps({
  cpu: { type: Object, required: true },
  cpuPoints: { type: Array, default: () => [] },
  stats: { type: Object, default: () => ({}) },
  liveRunning: { type: Boolean, default: false },
  liveAlertBanner: { type: String, default: '' },
  readOnlyRemote: { type: Boolean, default: false },
})

const emit = defineEmits(['dismiss-banner', 'copy-summary', 'save-session'])

const coresExpanded = ref(false)

const cpuTrendSvg = computed(() => {
  const pts = props.cpuPoints || []
  if (!pts.length) return null
  const w = 640
  const h = 128
  const padX = 12
  const padY = 12
  const innerW = w - padX * 2
  const innerH = h - padY * 2
  const n = pts.length
  const xAt = (i) => (n <= 1 ? padX + innerW / 2 : padX + (i / (n - 1)) * innerW)
  const yAt = (v) => padY + innerH * (1 - Math.max(0, Math.min(100, v)) / 100)

  const usageLine = pts.map((p, i) => `${xAt(i).toFixed(1)},${yAt(p.total_usage).toFixed(1)}`).join(' ')
  const tempPts = pts
    .map((p, i) => (p.temp_c !== null && p.temp_c !== undefined ? `${xAt(i).toFixed(1)},${yAt(p.temp_c).toFixed(1)}` : null))
    .filter(Boolean)
    .join(' ')
  const alerts = pts
    .map((p, i) => (p.alert ? { x: xAt(i), y: yAt(p.total_usage), ts: p.ts, usage: p.total_usage } : null))
    .filter(Boolean)
  return { w, h, usageLine, tempPts, alerts, firstTs: pts[0].ts, lastTs: pts[n - 1].ts }
})
</script>

<template>
  <div class="flex flex-col gap-3">
    <div v-if="liveAlertBanner" class="flex items-center justify-between gap-2 rounded-lg border border-rose-500/40 bg-rose-500/10 px-3 py-2 text-xs text-rose-300">
      <span class="truncate font-medium">{{ tl('实时告警：', 'Live Alert: ') }}{{ liveAlertBanner }}</span>
      <button @click="emit('dismiss-banner')" class="shrink-0 text-rose-400 hover:text-white">{{ tl('关闭', 'Dismiss') }}</button>
    </div>

    <div class="flex flex-wrap items-center justify-between gap-2">
      <div class="text-xs text-slate-400">
        {{ liveRunning ? t.liveRunningDesc : t.liveStoppedDesc }}
        <span v-if="stats?.cpu_usage" class="ml-2 font-mono text-slate-300">
          · Avg {{ stats.cpu_usage.avg }}% / Peak {{ stats.cpu_usage.max }}%
        </span>
      </div>
      <div class="flex items-center gap-2">
        <button @click="emit('copy-summary')" class="rounded-lg border border-sky-500/30 bg-sky-500/10 px-2.5 py-1 text-xs font-medium text-sky-300 hover:bg-sky-500/20">
          {{ t.copySummary }}
        </button>
        <button
          v-if="!liveRunning && !readOnlyRemote"
          @click="emit('save-session')"
          class="rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-2.5 py-1 text-xs font-medium text-emerald-300 hover:bg-emerald-500/20"
        >
          {{ t.saveSession }}
        </button>
      </div>
    </div>

    <!-- 首屏核心概览：总圆环 + P/E 核 -->
    <div class="grid grid-cols-1 gap-3 md:grid-cols-3">
      <div class="flex items-center gap-3.5 rounded-xl border border-white/[0.07] bg-[#0b0f17] p-3.5" :class="!cpu.has_hybrid ? 'md:col-span-3' : ''">
        <div class="relative h-16 w-16 shrink-0">
          <svg class="h-16 w-16 -rotate-90" viewBox="0 0 64 64">
            <circle cx="32" cy="32" r="26" fill="none" stroke="rgba(255,255,255,0.08)" stroke-width="6" />
            <circle
              cx="32" cy="32" r="26" fill="none" stroke="#38bdf8" stroke-width="6" stroke-linecap="round"
              :stroke-dasharray="163.36"
              :stroke-dashoffset="163.36 * (1 - Math.min(100, cpu.total_usage) / 100)"
              class="transition-all duration-300"
            />
          </svg>
          <div class="absolute inset-0 flex items-center justify-center font-mono text-xs font-bold text-white">
            {{ cpu.total_usage.toFixed(0) }}%
          </div>
        </div>
        <div class="min-w-0 flex-1">
          <div class="truncate text-xs font-bold text-white">{{ cpu.brand }}</div>
          <div class="mt-1 font-mono text-xs text-slate-300">
            {{ tl('温度', 'Temp') }}
            <strong :class="(cpu.temp_c || 0) >= 85 ? 'text-rose-400' : (cpu.temp_c || 0) >= 75 ? 'text-amber-400' : 'text-sky-300'">
              {{ cpu.temp_c !== null && cpu.temp_c !== undefined ? `${cpu.temp_c.toFixed(1)}°C` : '—' }}
            </strong>
            · {{ tl(`${cpu.physical_cores}核${cpu.logical_cores}框`, `${cpu.physical_cores}C/${cpu.logical_cores}T`) }}
          </div>
          <div class="mt-0.5 font-mono text-[11px] text-slate-400">
            {{ tl('均频', 'Avg') }} {{ cpu.avg_freq_mhz }} MHz · {{ tl('峰值', 'Max') }} {{ cpu.max_freq_mhz }} MHz
          </div>
        </div>
      </div>

      <div v-if="cpu.has_hybrid" class="flex items-center gap-3 rounded-xl border border-blue-500/20 bg-[#0b0f17] p-3.5">
        <div class="relative h-14 w-14 shrink-0">
          <svg class="h-14 w-14 -rotate-90" viewBox="0 0 56 56">
            <circle cx="28" cy="28" r="22" fill="none" stroke="rgba(255,255,255,0.08)" stroke-width="5" />
            <circle
              cx="28" cy="28" r="22" fill="none" stroke="#60a5fa" stroke-width="5" stroke-linecap="round"
              :stroke-dasharray="138.23"
              :stroke-dashoffset="138.23 * (1 - Math.min(100, cpu.p_usage) / 100)"
              class="transition-all duration-300"
            />
          </svg>
          <div class="absolute inset-0 flex items-center justify-center font-mono text-[11px] font-bold text-blue-300">
            {{ cpu.p_usage.toFixed(0) }}%
          </div>
        </div>
        <div>
          <div class="text-xs font-bold text-blue-400">{{ tl('P 性能核', 'P-Core') }} ×{{ cpu.p_count }}</div>
          <div class="mt-1 font-mono text-xs text-slate-300">{{ tl('平均', 'Avg') }} {{ cpu.p_freq_mhz }} MHz</div>
        </div>
      </div>

      <div v-if="cpu.has_hybrid" class="flex items-center gap-3 rounded-xl border border-emerald-500/20 bg-[#0b0f17] p-3.5">
        <div class="relative h-14 w-14 shrink-0">
          <svg class="h-14 w-14 -rotate-90" viewBox="0 0 56 56">
            <circle cx="28" cy="28" r="22" fill="none" stroke="rgba(255,255,255,0.08)" stroke-width="5" />
            <circle
              cx="28" cy="28" r="22" fill="none" stroke="#34d399" stroke-width="5" stroke-linecap="round"
              :stroke-dasharray="138.23"
              :stroke-dashoffset="138.23 * (1 - Math.min(100, cpu.e_usage) / 100)"
              class="transition-all duration-300"
            />
          </svg>
          <div class="absolute inset-0 flex items-center justify-center font-mono text-[11px] font-bold text-emerald-300">
            {{ cpu.e_usage.toFixed(0) }}%
          </div>
        </div>
        <div>
          <div class="text-xs font-bold text-emerald-400">{{ tl('E 能效核', 'E-Core') }} ×{{ cpu.e_count }}</div>
          <div class="mt-1 font-mono text-xs text-slate-300">{{ tl('平均', 'Avg') }} {{ cpu.e_freq_mhz }} MHz</div>
        </div>
      </div>
    </div>

    <!-- 首屏中区：60 秒实时趋势与异常点标注 -->
    <div v-if="cpuTrendSvg" class="rounded-xl border border-white/[0.07] bg-[#0b0f17] p-3">
      <div class="mb-2 flex flex-wrap items-center justify-between gap-2 text-xs">
        <span class="font-bold text-white">{{ t.cpuTrendTitle }}</span>
        <span class="font-mono text-[11px] text-slate-400">
          {{ cpuTrendSvg.firstTs }} ~ {{ cpuTrendSvg.lastTs }} · {{ tl('蓝线占用% · 橙线温度°C · 红点异常', 'Blue: Usage% · Orange: Temp°C · Red: Alert') }}
        </span>
      </div>
      <svg :viewBox="`0 0 ${cpuTrendSvg.w} ${cpuTrendSvg.h}`" class="h-32 w-full">
        <line x1="12" y1="12" x2="628" y2="12" stroke="rgba(255,255,255,0.06)" stroke-dasharray="3 3" />
        <line x1="12" y1="64" x2="628" y2="64" stroke="rgba(255,255,255,0.06)" stroke-dasharray="3 3" />
        <line x1="12" y1="116" x2="628" y2="116" stroke="rgba(255,255,255,0.08)" />
        <polyline v-if="cpuTrendSvg.tempPts" fill="none" stroke="#f59e0b" stroke-width="1.8" :points="cpuTrendSvg.tempPts" />
        <polyline fill="none" stroke="#38bdf8" stroke-width="2.2" :points="cpuTrendSvg.usageLine" />
        <circle
          v-for="(a, i) in cpuTrendSvg.alerts"
          :key="i"
          :cx="a.x" :cy="a.y" r="4" fill="#f43f5e" stroke="#0b0f17" stroke-width="1.5"
        >
          <title>{{ tl(`${a.ts} 触发异常/降频（占用 ${a.usage.toFixed(1)}%）`, `${a.ts} Alert/Throttle (Usage ${a.usage.toFixed(1)}%)`) }}</title>
        </circle>
      </svg>
    </div>

    <!-- 首屏下区：Top 5 CPU / 内存进程榜 -->
    <div v-if="(cpu.top_cpu_procs && cpu.top_cpu_procs.length) || (cpu.top_mem_procs && cpu.top_mem_procs.length)" class="grid grid-cols-1 gap-3 md:grid-cols-2">
      <div class="rounded-xl border border-white/[0.07] bg-[#0b0f17] p-3">
        <div class="mb-2 text-xs font-bold text-sky-400">{{ t.topCpuProcs }}</div>
        <div class="flex flex-col gap-1.5 font-mono text-xs">
          <div v-for="p in cpu.top_cpu_procs || []" :key="`cpu-${p.pid}`" class="flex items-center justify-between gap-2">
            <span class="truncate text-slate-200">{{ p.name }} <span class="text-slate-500">({{ p.pid }})</span></span>
            <span class="shrink-0 text-sky-300">{{ p.cpu_pct.toFixed(1) }}% · {{ p.mem_mb }} MB</span>
          </div>
        </div>
      </div>
      <div class="rounded-xl border border-white/[0.07] bg-[#0b0f17] p-3">
        <div class="mb-2 text-xs font-bold text-emerald-400">{{ t.topMemProcs }}</div>
        <div class="flex flex-col gap-1.5 font-mono text-xs">
          <div v-for="p in cpu.top_mem_procs || []" :key="`mem-${p.pid}`" class="flex items-center justify-between gap-2">
            <span class="truncate text-slate-200">{{ p.name }} <span class="text-slate-500">({{ p.pid }})</span></span>
            <span class="shrink-0 text-emerald-300">{{ p.mem_mb }} MB · {{ p.cpu_pct.toFixed(1) }}%</span>
          </div>
        </div>
      </div>
    </div>

    <!-- 可折叠逐核环形进度条网格 -->
    <div class="flex flex-col gap-2">
      <button
        @click="coresExpanded = !coresExpanded"
        class="self-start rounded-lg border border-white/[0.08] bg-white/[0.03] px-3 py-1.5 text-xs text-slate-300 hover:bg-white/[0.06]"
      >
        {{ coresExpanded ? `▼ ${t.collapseCores}` : `▶ ${t.expandCores}` }}（{{ cpu.cores.length }}）
      </button>
      <div v-if="coresExpanded" class="grid grid-cols-3 gap-2 sm:grid-cols-4 md:grid-cols-6 lg:grid-cols-8 xl:grid-cols-10">
        <div
          v-for="c in cpu.cores"
          :key="c.id"
          class="flex flex-col items-center rounded-xl border border-white/[0.06] bg-[#0b0f17] p-2.5 font-mono"
        >
          <div class="relative h-12 w-12">
            <svg class="h-12 w-12 -rotate-90" viewBox="0 0 48 48">
              <circle cx="24" cy="24" r="19" fill="none" stroke="rgba(255,255,255,0.08)" stroke-width="4" />
              <circle
                cx="24" cy="24" r="19" fill="none"
                :stroke="c.kind === 'P' ? '#60a5fa' : c.kind === 'E' ? '#34d399' : '#38bdf8'"
                stroke-width="4" stroke-linecap="round"
                :stroke-dasharray="119.38"
                :stroke-dashoffset="119.38 * (1 - Math.min(100, c.usage) / 100)"
                class="transition-all duration-300"
              />
            </svg>
            <div class="absolute inset-0 flex items-center justify-center text-[10px] font-bold text-white">
              {{ c.usage.toFixed(0) }}%
            </div>
          </div>
          <div
            class="mt-1.5 text-[10.5px] font-bold"
            :class="c.kind === 'P' ? 'text-blue-400' : c.kind === 'E' ? 'text-emerald-400' : 'text-sky-400'"
          >
            C{{ c.id }} · {{ c.kind }}
          </div>
          <div class="text-[10px] text-slate-400">{{ c.freq_mhz }} MHz</div>
        </div>
      </div>
    </div>
  </div>
</template>
