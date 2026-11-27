<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import * as echarts from 'echarts'

const props = defineProps({
  name: { type: String, required: true },
  unit: { type: String, required: true },
  decimals: { type: Number, default: 0 },
  color: { type: String, required: true },
  points: { type: Array, required: true },
  alertTimes: { type: Array, default: () => [] },
})

const chartEl = ref(null)
let chart = null
let ro = null

const fmt = (v) =>
  v === null || v === undefined || Number.isNaN(v) ? '—' : Number(v).toFixed(props.decimals)

const stats = computed(() => {
  if (!props.points.length) return { cur: '—', max: '—', min: '—' }
  let lo = Infinity
  let hi = -Infinity
  for (const p of props.points) {
    if (p.val < lo) lo = p.val
    if (p.val > hi) hi = p.val
  }
  const last = props.points[props.points.length - 1].val
  return { cur: fmt(last), max: fmt(hi), min: fmt(lo) }
})

function buildOption() {
  const xs = props.points.map((p) => p.ts.slice(3) || p.ts)
  const ys = props.points.map((p) => p.val)
  const alertSet = new Set(props.alertTimes)
  const markLines = []
  props.points.forEach((p, idx) => {
    if (alertSet.has(p.ts)) {
      markLines.push({ xAxis: idx })
    }
  })

  return {
    animation: false,
    grid: { left: 42, right: 14, top: 28, bottom: 24 },
    tooltip: {
      trigger: 'axis',
      backgroundColor: 'rgba(15, 20, 30, 0.94)',
      borderColor: 'rgba(255, 255, 255, 0.12)',
      borderWidth: 1,
      padding: [8, 12],
      textStyle: { color: '#e2e8f0', fontSize: 12 },
      formatter: (params) => {
        const it = params?.[0]
        if (!it) return ''
        const rawTs = props.points[it.dataIndex]?.ts || it.name
        return `<div style="color:#94a3b8;font-size:11px;margin-bottom:2px">${rawTs}</div>
                <span style="color:${props.color}">●</span> ${props.name}：<b style="color:#fff">${fmt(it.value)} ${props.unit}</b>`
      },
    },
    xAxis: {
      type: 'category',
      data: xs,
      boundaryGap: true,
      axisLine: { lineStyle: { color: 'rgba(255,255,255,0.08)' } },
      axisTick: { show: false },
      axisLabel: { color: '#64748b', fontSize: 11, hideOverlap: true },
      splitLine: { show: false },
    },
    yAxis: {
      type: 'value',
      scale: true,
      axisLabel: { color: '#64748b', fontSize: 11 },
      splitLine: { lineStyle: { color: 'rgba(255,255,255,0.05)', type: 'dashed' } },
    },
    series: [
      {
        name: props.name,
        type: 'line',
        data: ys,
        showSymbol: true,
        symbol: 'circle',
        symbolSize: 6,
        smooth: 0.15,
        lineStyle: {
          color: props.color,
          width: 2.2,
          shadowColor: `${props.color}44`,
          shadowBlur: 8,
        },
        itemStyle: {
          color: props.color,
          borderColor: '#121824',
          borderWidth: 1.5,
        },
        areaStyle: {
          color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
            { offset: 0, color: `${props.color}45` },
            { offset: 1, color: `${props.color}02` },
          ]),
        },
        label: {
          show: true,
          position: 'top',
          distance: 4,
          color: '#cbd5e1',
          fontSize: 10,
          formatter: (p) => fmt(p.value),
        },
        markLine: markLines.length
          ? {
              silent: true,
              symbol: ['none', 'none'],
              lineStyle: { color: '#ef4444', type: 'dashed', width: 1.2 },
              label: { show: false },
              data: markLines,
            }
          : undefined,
      },
    ],
  }
}

onMounted(() => {
  if (!chartEl.value) return
  chart = echarts.init(chartEl.value, null, { renderer: 'canvas' })
  chart.setOption(buildOption())
  ro = new ResizeObserver(() => chart?.resize())
  ro.observe(chartEl.value)
})

watch(
  () => [props.points, props.alertTimes],
  () => {
    chart?.setOption(buildOption(), { notMerge: true })
  },
  { deep: false }
)

onBeforeUnmount(() => {
  ro?.disconnect()
  chart?.dispose()
})
</script>

<template>
  <div class="group rounded-xl border border-white/[0.07] bg-[#121824] p-3.5 shadow-sm transition hover:border-white/[0.15]">
    <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
      <div class="flex items-center gap-2">
        <span class="inline-block h-4 w-1 rounded-full" :style="{ backgroundColor: color }"></span>
        <span class="text-sm font-semibold tracking-tight text-slate-100">{{ name }}</span>
        <span class="rounded bg-white/[0.05] px-1.5 py-0.5 text-[11px] text-slate-400">{{ unit }}</span>
      </div>
      <div class="flex items-center gap-2.5 text-xs">
        <span class="font-mono text-sm font-bold" :style="{ color }">{{ stats.cur }} {{ unit }}</span>
        <div v-if="points.length" class="flex items-center gap-1.5 rounded-md bg-white/[0.04] px-2 py-0.5 font-mono text-[11px] text-slate-400">
          <span>高 {{ stats.max }}</span>
          <span class="text-slate-600">/</span>
          <span>低 {{ stats.min }}</span>
        </div>
      </div>
    </div>
    <div v-if="!points.length" class="flex h-44 items-center justify-center text-sm text-slate-500">
      该参数暂无有效数据
    </div>
    <div v-else ref="chartEl" class="h-44 w-full"></div>
  </div>
</template>
