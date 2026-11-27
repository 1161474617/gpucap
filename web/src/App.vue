<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { Toaster, toast } from 'vue-sonner'
import 'vue-sonner/style.css'
import QRCode from 'qrcode'
import CpuMonitorPanel from './components/CpuMonitorPanel.vue'
import { currentLang, setAppLang, showLangPicker, t, tl, trBackendText, trThr } from './i18n'
import {
  Activity,
  AlertTriangle,
  ArrowDownToLine,
  Copy,
  Cpu,
  Download,
  FileWarning,
  FolderOpen,
  Gamepad2,
  HardDrive,
  LineChart,
  Pause,
  Play,
  QrCode,
  RefreshCw,
  Search,
  Settings,
  ShieldAlert,
  Smartphone,
  Square,
  Terminal,
  Trash2,
  X,
} from 'lucide-vue-next'
import TrendCard from './components/TrendCard.vue'

const activeTab = ref('gpu')
const paused = ref(false)
const onlyBad = ref(false)
const fullHistory = ref(false)
const logFilter = ref('')
const trendWindow = ref(20)
const selectedSession = ref('')
const manualSessionLock = ref(false)

const state = ref({
  collector_running: false,
  live_running: false,
  live_has_data: false,
  cpu: null,
  cpu_points: [],
  stats: {},
  env: null,
  read_only_remote: false,
  heartbeat: { text: '● Connecting...', level: 'dim' },
  session: '',
  sessions: [],
  gpu_summary: '—',
  mem_summary: '—',
  proc_summary: '—',
  gpu_rows: [],
  mem_points: [],
  events: [],
  wer: [],
  alerts: [],
  alert_count: 0,
  startup_enabled: false,
  full_history: false,
  lan_url: '',
  local_ipv6_url: '',
  ipv6_url: '',
})

const incidents = ref([])
const selectedIncident = ref('')
const previewFile = ref('incident.json')
const previewText = ref('')

const showSettings = ref(false)
const settingsForm = ref({
  gpu_index: 0,
  game_names_text: '',
  temp_warn: 78,
  hotspot_warn: 92,
  hotspot_delta_warn: 20,
  power_warn: 360,
  cpu_temp_warn: 90,
  cpu_usage_warn: 95,
  vram_warn_pct: 92,
  pcie_drop_warn: true,
  gpu_interval: 1,
  mem_interval: 5,
  proc_interval: 5,
  wer_interval: 5,
  max_session_hours: 24,
  rotate_mb: 64,
  minimize_to_tray: true,
  auth_pin: '',
  read_only_remote: false,
})
const gpuList = ref([])

const showBatchDelete = ref(false)
const batchSelected = ref(new Set())

// 手机扫码弹窗
const showQr = ref(false)
const qrDataUrl = ref('')

// 日志详情弹窗（方便手机端阅读长消息）
const logDetail = ref(null)

const gpuTableWrap = ref(null)
let pollTimer = null

function notify(msg, type = 'ok') {
  if (type === 'bad') toast.error(msg)
  else if (type === 'warn') toast.warning(msg)
  else toast.success(msg)
}

const authPin = ref(localStorage.getItem('gpucap_pin') || '')
const showPinModal = ref(false)
const pinInput = ref('')
const pinError = ref('')

async function apiJson(url, opts = {}) {
  const headers = { ...(opts.headers || {}) }
  if (authPin.value) {
    headers['X-GPUCap-Pin'] = authPin.value
  }
  const res = await fetch(url, { ...opts, headers })
  const data = await res.json().catch(() => ({}))
  if (res.status === 401 && data.auth_required) {
    showPinModal.value = true
    throw new Error(data.error || tl('需要访问口令', 'Access PIN required'))
  }
  if (!res.ok) throw new Error(data.error || tl(`请求失败 ${res.status}`, `Request failed ${res.status}`))
  return data
}

async function verifyPin() {
  pinError.value = ''
  try {
    await apiJson('/api/auth/verify', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ pin: pinInput.value }),
    })
    authPin.value = pinInput.value.trim()
    localStorage.setItem('gpucap_pin', authPin.value)
    showPinModal.value = false
    pinInput.value = ''
    await fetchState(true)
  } catch (e) {
    pinError.value = e.message || tl('口令错误', 'Invalid PIN')
  }
}

const lastNotifiedAlerts = ref(0)
const liveAlertBanner = ref('')

async function copyLiveSummary() {
  try {
    const data = await apiJson('/api/live/summary')
    if (navigator.clipboard && data.summary) {
      await navigator.clipboard.writeText(data.summary)
      notify(tl('已复制诊断摘要到剪贴板', 'Diagnostic summary copied'))
    }
  } catch (e) {
    notify(e.message, 'bad')
  }
}

async function saveLiveSession() {
  try {
    const data = await apiJson('/api/live/save', { method: 'POST' })
    notify(tl(`已补存为会话：${data.session}`, `Saved as session: ${data.session}`))
    selectedSession.value = data.session
    await fetchState(true)
  } catch (e) {
    notify(e.message, 'bad')
  }
}

async function fetchState(force = false) {
  if (paused.value && !force) return
  try {
    const prevLive = state.value.live_running
    const prevCollector = state.value.collector_running
    const q = new URLSearchParams()
    if (selectedSession.value) q.set('session', selectedSession.value)
    if (fullHistory.value) q.set('full', '1')
    const data = await apiJson(`/api/state?${q.toString()}`)

    // 检测主机端刚开启实时监测或后台采集，自动解除旧会话锁定并立即跟随
    const activeSess = (data.sessions || []).find((s) => s.active && s.name !== '__live__')?.name || ''
    let needRefetch = false
    if ((!prevLive && data.live_running) || (!manualSessionLock.value && data.live_running && data.session !== '__live__')) {
      manualSessionLock.value = false
      selectedSession.value = '__live__'
      paused.value = false
      if (data.session !== '__live__') needRefetch = true
    } else if (!prevCollector && data.collector_running) {
      manualSessionLock.value = false
      selectedSession.value = activeSess
      paused.value = false
      if (activeSess && data.session !== activeSess) needRefetch = true
    } else if (!manualSessionLock.value && data.collector_running && activeSess && data.session !== activeSess) {
      selectedSession.value = activeSess
      needRefetch = true
    }

    const nextAlertCount = data.alert_count || 0
    if (nextAlertCount > lastNotifiedAlerts.value && (lastNotifiedAlerts.value > 0 || data.live_running)) {
      const latest = (data.alerts || [])[data.alerts.length - 1]
      if (latest) {
        liveAlertBanner.value = `[${latest.time}] ${latest.source}: ${latest.message}`
        notify(tl(`新告警：${latest.message}`, `New Alert: ${latest.message}`), 'bad')
      }
    }
    lastNotifiedAlerts.value = nextAlertCount
    state.value = data
    if (!selectedSession.value && data.session) {
      selectedSession.value = data.session
    }
    if (needRefetch && !force) {
      await fetchState(true)
      return
    }
    if (!paused.value && activeTab.value === 'gpu') {
      await nextTick()
      if (gpuTableWrap.value) {
        gpuTableWrap.value.scrollTop = gpuTableWrap.value.scrollHeight
      }
    }
  } catch {
    // 忽略瞬时网络抖动
  }
}

async function fetchIncidents() {
  try {
    const q = selectedSession.value
      ? `?session=${encodeURIComponent(selectedSession.value)}`
      : ''
    const data = await apiJson(`/api/incidents${q}`)
    incidents.value = data.items || []
    if (incidents.value.length && !incidents.value.some((i) => i.name === selectedIncident.value)) {
      selectedIncident.value = incidents.value[0].name
    }
    if (selectedIncident.value) {
      await loadIncidentPreview()
    } else {
      previewText.value = ''
    }
  } catch {
    // 忽略
  }
}

async function loadIncidentPreview() {
  if (!selectedIncident.value) return
  try {
    const q = new URLSearchParams({
      name: selectedIncident.value,
      file: previewFile.value,
    })
    const data = await apiJson(`/api/incidents/preview?${q.toString()}`)
    previewText.value = data.text || ''
  } catch {
    previewText.value = tl('(读取失败)', '(Load failed)')
  }
}

async function openQrModal() {
  const v4 = state.value.lan_url || window.location.origin
  const v6 = state.value.ipv6_url || ''
  const payload = v6 ? JSON.stringify({ gpucap: 1, ipv4: v4, ipv6: v6 }) : v4
  try {
    qrDataUrl.value = await QRCode.toDataURL(payload, {
      width: 220,
      margin: 2,
      color: { dark: '#e2e8f0', light: '#0b0f17' },
    })
    showQr.value = true
  } catch {
    notify(tl('生成二维码失败', 'Failed to generate QR code'), 'bad')
  }
}

async function toggleCollector() {
  try {
    if (state.value.collector_running) {
      await apiJson('/api/collector/stop', { method: 'POST' })
      notify(tl('已发送停止采集信号', 'Stop capture signal sent'), 'warn')
    } else {
      await apiJson('/api/collector/start', { method: 'POST' })
      notify(tl('后台采集已启动', 'Background capture started'))
      setTimeout(() => {
        selectedSession.value = ''
        fetchState(true)
      }, 1200)
    }
    await fetchState(true)
  } catch (e) {
    notify(e.message, 'bad')
  }
}

async function toggleLiveMonitor() {
  try {
    if (state.value.live_running) {
      await apiJson('/api/live/stop', { method: 'POST' })
      notify(tl('已停止实时监测（保留内存日志）', 'Live monitor stopped (logs retained)'), 'warn')
    } else {
      await apiJson('/api/live/start', { method: 'POST' })
      selectedSession.value = '__live__'
      notify(tl('已开启纯内存实时监测（不保存磁盘）', 'Live monitor started (in-memory only)'))
    }
    await fetchState(true)
  } catch (e) {
    notify(e.message, 'bad')
  }
}

async function toggleStartup() {
  try {
    const next = !state.value.startup_enabled
    await apiJson('/api/startup', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ enabled: next }),
    })
    state.value.startup_enabled = next
    notify(next ? tl('已开启开机自启', 'Launch at startup enabled') : tl('已关闭开机自启', 'Launch at startup disabled'))
  } catch (e) {
    notify(e.message, 'bad')
  }
}

async function openSettings() {
  try {
    const data = await apiJson('/api/config')
    gpuList.value = data.gpus || []
    settingsForm.value = {
      ...data.config,
      game_names_text: (data.config.game_names || []).join(', '),
    }
    showSettings.value = true
  } catch (e) {
    notify(e.message, 'bad')
  }
}

async function saveSettings() {
  try {
    const payload = {
      ...settingsForm.value,
      game_names: settingsForm.value.game_names_text
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean),
    }
    delete payload.game_names_text
    await apiJson('/api/config', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    })
    showSettings.value = false
    notify(tl('设置已保存，下次启动采集生效', 'Settings saved (effective on next capture)'))
  } catch (e) {
    notify(e.message, 'bad')
  }
}

function onPickSession(name) {
  const picked = (state.value.sessions || []).find((s) => s.name === name)
  manualSessionLock.value = !(name === '__live__' || picked?.active)
  selectedSession.value = name
  fullHistory.value = false
  logFilter.value = ''
  fetchState(true)
  fetchIncidents()
}

function loadFullHistory() {
  fullHistory.value = true
  fetchState(true)
}

function exportZip() {
  if (!state.value.session) return
  window.open(`/api/sessions/export?session=${encodeURIComponent(state.value.session)}`, '_blank')
  notify(tl('正在下载会话打包压缩包', 'Downloading session ZIP'))
}

function jumpToEnd() {
  paused.value = false
  if (activeTab.value === 'trend') {
    trendWindow.value = 30
  }
  fetchState(true)
}

async function openFolder(kind) {
  try {
    await apiJson('/api/open-folder', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ kind, session: state.value.session }),
    })
    notify(tl('已在主机资源管理器中打开', 'Opened in host Explorer'))
  } catch (e) {
    notify(e.message, 'bad')
  }
}

function copyText(txt, tip = tl('已复制到剪贴板', 'Copied to clipboard')) {
  if (navigator.clipboard?.writeText) {
    navigator.clipboard.writeText(txt).then(() => notify(tip)).catch(() => notify(tl('复制失败', 'Copy failed'), 'bad'))
  } else {
    notify(tl('当前浏览器上下文不支持剪贴板写入', 'Clipboard not supported in this context'), 'warn')
  }
}

function openBatchDelete() {
  batchSelected.value = new Set()
  showBatchDelete.value = true
}

function batchSelectAll() {
  const s = new Set()
  for (const it of state.value.sessions) {
    if (!it.active) s.add(it.name)
  }
  batchSelected.value = s
}

function batchSelectOthers() {
  const s = new Set()
  for (const it of state.value.sessions) {
    if (!it.active && it.name !== state.value.session) s.add(it.name)
  }
  batchSelected.value = s
}

function batchInvert() {
  const s = new Set(batchSelected.value)
  for (const it of state.value.sessions) {
    if (it.active) continue
    if (s.has(it.name)) s.delete(it.name)
    else s.add(it.name)
  }
  batchSelected.value = s
}

function toggleBatchItem(name) {
  const s = new Set(batchSelected.value)
  if (s.has(name)) s.delete(name)
  else s.add(name)
  batchSelected.value = s
}

async function confirmBatchDelete() {
  const names = Array.from(batchSelected.value)
  if (!names.length) return
  try {
    const res = await apiJson('/api/sessions/delete', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ names }),
    })
    showBatchDelete.value = false
    if (names.includes(selectedSession.value)) {
      selectedSession.value = ''
    }
    notify(tl(`已删除 ${res.deleted_sessions} 个会话（含 ${res.deleted_incidents} 个事故目录）`, `Deleted ${res.deleted_sessions} sessions (${res.deleted_incidents} incidents)`))
    await fetchState(true)
    await fetchIncidents()
  } catch (e) {
    notify(e.message, 'bad')
  }
}

const hotspotDelta = (r) => {
  if (!r) return null
  const hs = Number(r.hotspot)
  const t = Number(r.temp)
  if (!Number.isFinite(hs) || !Number.isFinite(t) || r.hotspot === '' || r.temp === '') return null
  return (hs - t).toFixed(1)
}

const latestGpu = computed(() => {
  const rows = state.value.gpu_rows || []
  return rows.length ? rows[rows.length - 1] : null
})

const latestMem = computed(() => {
  const pts = state.value.mem_points || []
  const pct = pts.length ? pts[pts.length - 1].commit_pct : 0
  return { text: state.value.mem_summary || '—', pct: Math.min(100, Math.max(0, pct)) }
})

const filteredGpuRows = computed(() => {
  if (!onlyBad.value) return state.value.gpu_rows
  return state.value.gpu_rows.filter((r) => r.sev !== 'normal')
})

function filterLogs(list, applyOnlyBad) {
  const kw = logFilter.value.trim().toLowerCase()
  return (list || []).filter((item) => {
    if (applyOnlyBad && item.sev === 'normal') return false
    if (!kw) return true
    return (
      item.time.toLowerCase().includes(kw) ||
      item.level.toLowerCase().includes(kw) ||
      item.source.toLowerCase().includes(kw) ||
      item.message.toLowerCase().includes(kw)
    )
  })
}

const filteredEvents = computed(() => filterLogs(state.value.events, onlyBad.value))
const filteredWer = computed(() => filterLogs(state.value.wer, false))
const filteredAlerts = computed(() => filterLogs(state.value.alerts, false))

const trendSlice = computed(() => state.value.gpu_rows.slice(-trendWindow.value))
const alertTimes = computed(() => (state.value.alerts || []).map((a) => a.time))

const trendCards = computed(() => {
  const rows = trendSlice.value
  const pickNum = (val) => {
    const n = parseFloat(val)
    return Number.isNaN(n) ? null : n
  }
  const mapGpu = (fn) =>
    rows
      .map((r) => ({ ts: r.ts, val: fn(r) }))
      .filter((p) => p.val !== null)

  const mRows = (state.value.mem_points || []).slice(-Math.min(trendWindow.value, 30))

  return [
    { name: tl('GPU 温度', 'GPU Temp'), unit: '°C', decimals: 1, color: '#fb7185', points: mapGpu((r) => pickNum(r.temp)) },
    { name: tl('GPU 热点温度', 'GPU Hotspot'), unit: '°C', decimals: 1, color: '#f43f5e', points: mapGpu((r) => pickNum(r.hotspot)) },
    {
      name: tl('热点温差 (热点-GPU)', 'Hotspot Delta'),
      unit: '°C',
      decimals: 1,
      color: '#fb923c',
      points: mapGpu((r) => {
        const hs = pickNum(r.hotspot)
        const t = pickNum(r.temp)
        return hs !== null && t !== null ? Number((hs - t).toFixed(2)) : null
      }),
    },
    { name: tl('功耗', 'Power'), unit: 'W', decimals: 1, color: '#f59e0b', points: mapGpu((r) => pickNum(r.power)) },
    { name: tl('核心频率', 'Core Clock'), unit: 'MHz', decimals: 0, color: '#38bdf8', points: mapGpu((r) => pickNum(r.core)) },
    { name: tl('显存频率', 'Mem Clock'), unit: 'MHz', decimals: 0, color: '#a78bfa', points: mapGpu((r) => pickNum(r.memclk)) },
    { name: tl('GPU 占用', 'GPU Util'), unit: '%', decimals: 0, color: '#34d399', points: mapGpu((r) => pickNum(r.util)) },
    { name: tl('显存占用', 'VRAM Used'), unit: 'MB', decimals: 0, color: '#2dd4bf', points: mapGpu((r) => pickNum(r.vram)) },
    { name: tl('风扇转速', 'Fan Speed'), unit: '%', decimals: 0, color: '#f472b6', points: mapGpu((r) => pickNum(r.fan)) },
    {
      name: tl('PCIe 代数', 'PCIe Gen'),
      unit: 'Gen',
      decimals: 0,
      color: '#f87171',
      points: mapGpu((r) => {
        const m = /^Gen(\d+)/.exec(r.pcie || '')
        return m ? Number(m[1]) : null
      }),
    },
    {
      name: tl('系统内存提交率', 'Commit Charge'),
      unit: '%',
      decimals: 1,
      color: '#4ade80',
      points: mRows.map((m) => ({ ts: m.ts, val: m.commit_pct })),
    },
    {
      name: tl('非分页内核池', 'Non-Paged Pool'),
      unit: 'MB',
      decimals: 0,
      color: '#fb923c',
      points: mRows.map((m) => ({ ts: m.ts, val: m.nonpaged_mb })),
    },
  ]
})

const hbBadge = computed(() => {
  const lv = state.value.heartbeat?.level
  if (lv === 'ok') return { cls: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-400', dot: 'bg-emerald-400', pulse: true }
  if (lv === 'warn') return { cls: 'border-amber-500/30 bg-amber-500/10 text-amber-400', dot: 'bg-amber-400', pulse: false }
  if (lv === 'bad') return { cls: 'border-rose-500/30 bg-rose-500/10 text-rose-400', dot: 'bg-rose-400', pulse: false }
  return { cls: 'border-white/10 bg-white/[0.04] text-slate-400', dot: 'bg-slate-400', pulse: false }
})

const sevColor = (sev) =>
  sev === 'bad' ? 'text-rose-400' : sev === 'warn' ? 'text-amber-400' : 'text-slate-200'

const badgeClass = (sev) =>
  sev === 'bad'
    ? 'bg-rose-500/20 text-rose-300 border border-rose-500/30'
    : sev === 'warn'
    ? 'bg-amber-500/20 text-amber-300 border border-amber-500/30'
    : 'bg-slate-700/40 text-slate-300 border border-white/10'

const tabs = computed(() => {
  const list = [
    { id: 'gpu', label: t.value.tabGpu, icon: Cpu, count: state.value.gpu_rows?.length || 0 },
  ]
  if (state.value.cpu) {
    list.push({ id: 'cpu', label: t.value.tabCpu, icon: Activity, count: state.value.cpu.logical_cores || 0 })
  }
  list.push(
    { id: 'trend', label: t.value.tabTrend, icon: LineChart, count: 0 },
    { id: 'events', label: t.value.tabEvents, icon: Terminal, count: state.value.events?.length || 0 },
    { id: 'wer', label: t.value.tabWer, icon: FileWarning, count: state.value.wer?.length || 0 },
    { id: 'alerts', label: t.value.tabAlerts, icon: ShieldAlert, count: state.value.alert_count || 0 },
  )
  return list
})

onMounted(() => {
  fetchState(true)
  fetchIncidents()
  pollTimer = setInterval(() => {
    fetchState(false)
    if (activeTab.value === 'alerts') fetchIncidents()
  }, 1000)
})

onBeforeUnmount(() => {
  clearInterval(pollTimer)
})
</script>

<template>
  <div class="relative flex min-h-screen flex-col bg-[#0b0f17] bg-[radial-gradient(ellipse_at_top,_rgba(56,189,248,0.09),_transparent_55%)] text-slate-200">
    <!-- vue-sonner 堆叠式悬浮通知 -->
    <Toaster theme="dark" position="top-right" rich-colors close-button />

    <!-- 吸顶毛玻璃顶栏 -->
    <header class="sticky top-0 z-30 border-b border-white/[0.06] bg-[#0f141e]/80 px-3 py-2.5 backdrop-blur-xl sm:px-5">
      <div class="mx-auto flex max-w-[1600px] flex-wrap items-center justify-between gap-2.5">
        <div class="flex flex-wrap items-center gap-2.5">
          <div class="flex items-center gap-2">
            <img src="/favicon.png" alt="GPUCap" class="h-7 w-7 rounded-lg ring-1 ring-sky-400/30" />
            <span class="bg-gradient-to-r from-white to-slate-400 bg-clip-text text-sm font-bold tracking-wide text-transparent">GPUCap</span>
          </div>

          <span class="inline-flex items-center gap-2 rounded-full border px-3 py-1 text-xs font-medium" :class="hbBadge.cls">
            <span class="relative flex h-2 w-2">
              <span v-if="hbBadge.pulse" class="absolute inline-flex h-full w-full animate-ping rounded-full opacity-75" :class="hbBadge.dot"></span>
              <span class="relative inline-flex h-2 w-2 rounded-full" :class="hbBadge.dot"></span>
            </span>
            <span>{{ trBackendText(state.heartbeat.text).replace(/^●\s*/, '') }}</span>
          </span>

          <span v-if="state.session" class="hidden rounded-md border border-white/[0.06] bg-white/[0.03] px-2 py-1 font-mono text-xs text-slate-400 sm:inline-block">
            {{ state.session }}
          </span>
        </div>

        <div class="flex flex-wrap items-center gap-2">
          <span
            v-if="state.alert_count > 0"
            @click="activeTab = 'alerts'"
            class="inline-flex cursor-pointer items-center gap-1.5 rounded-full border border-amber-500/30 bg-amber-500/10 px-2.5 py-1 text-xs font-semibold text-amber-300 transition hover:bg-amber-500/20"
          >
            <AlertTriangle class="h-3.5 w-3.5" />
            <span>{{ tl('告警', 'Alerts') }} {{ state.alert_count }}</span>
          </span>

          <button
            @click="setAppLang(currentLang === 'en' ? 'zh-CN' : 'en')"
            class="inline-flex items-center gap-1 rounded-lg border border-white/[0.08] bg-white/[0.04] px-2.5 py-1.5 text-xs font-medium text-slate-200 transition hover:bg-white/[0.08]"
          >
            {{ currentLang === 'en' ? '中文' : 'EN' }}
          </button>

          <button
            @click="openQrModal"
            class="inline-flex items-center gap-1.5 rounded-lg border border-white/[0.08] bg-white/[0.04] px-2.5 py-1.5 text-xs text-slate-200 transition hover:bg-white/[0.08]"
            title="Pair / QR"
          >
            <QrCode class="h-3.5 w-3.5 text-sky-400" />
            <span class="hidden sm:inline">{{ t.webQr }}</span>
          </button>

          <button
            @click="toggleCollector"
            class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-semibold text-white shadow-sm transition active:scale-95"
            :class="
              state.collector_running
                ? 'bg-rose-600 hover:bg-rose-500'
                : 'bg-emerald-600 hover:bg-emerald-500'
            "
          >
            <Square v-if="state.collector_running" class="h-3.5 w-3.5 fill-current" />
            <Play v-else class="h-3.5 w-3.5 fill-current" />
            <span>{{ state.collector_running ? tl('停止采集', 'Stop Cap') : tl('启动采集', 'Start Cap') }}</span>
          </button>

          <button
            @click="toggleLiveMonitor"
            class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-semibold text-white shadow-sm transition active:scale-95"
            :class="
              state.live_running
                ? 'bg-amber-600 hover:bg-amber-500'
                : 'bg-sky-600 hover:bg-sky-500'
            "
            :title="tl('纯内存实时监测，不保存任何文件到磁盘', 'In-memory live monitoring without saving to disk')"
          >
            <Square v-if="state.live_running" class="h-3.5 w-3.5 fill-current" />
            <Play v-else class="h-3.5 w-3.5 fill-current" />
            <span>{{ state.live_running ? tl('停止监测', 'Stop Live') : tl('实时监测', 'Live Mon') }}</span>
          </button>

          <button
            @click="openSettings"
            class="inline-flex items-center gap-1 rounded-lg border border-white/[0.08] bg-white/[0.04] px-2.5 py-1.5 text-xs text-slate-200 transition hover:bg-white/[0.08]"
          >
            <Settings class="h-3.5 w-3.5" />
            <span class="hidden sm:inline">{{ tl('设置', 'Settings') }}</span>
          </button>
        </div>
      </div>
    </header>

    <div class="mx-auto flex w-full max-w-[1600px] flex-1 flex-col gap-3 p-3 sm:p-4">
      <div v-if="state.env" class="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-white/[0.06] bg-[#121824]/80 px-3 py-2 text-xs text-slate-300">
        <span class="font-mono">
          {{ state.env.gpu_name }} · {{ tl('驱动', 'Driver') }} <strong class="text-sky-300">{{ state.env.driver_version || '—' }}</strong> · VBIOS {{ state.env.vbios || '—' }} · {{ state.env.pcie_max }}
        </span>
        <span class="font-mono text-slate-400">
          {{ state.env.os_version }} · {{ tl('电源：', 'Power: ') }}{{ state.env.power_plan }}
          <span v-if="state.env.driver_changed_from" class="ml-2 text-amber-400">{{ tl(`（驱动变更：${state.env.driver_changed_from} → ${state.env.driver_version}）`, `(Driver changed: ${state.env.driver_changed_from} → ${state.env.driver_version})`) }}</span>
        </span>
      </div>
      <!-- 4 枚现代化实时 KPI 卡片（手机 2x2，桌面 4 列） -->
      <section class="grid grid-cols-2 gap-2.5 lg:grid-cols-4">
        <div class="relative overflow-hidden rounded-xl border border-white/[0.07] bg-[#121824]/90 p-3.5 transition hover:border-white/[0.14]">
          <div class="flex items-center justify-between text-xs text-slate-400">
            <span>{{ tl('显卡温度 / 功耗', 'GPU Temp / Power') }}</span>
            <span class="rounded-lg bg-rose-500/10 p-1 text-rose-400"><Cpu class="h-3.5 w-3.5" /></span>
          </div>
          <div class="mt-1.5 flex flex-wrap items-baseline gap-x-2 gap-y-0.5 font-mono">
            <span class="text-lg font-bold text-white sm:text-xl">{{ latestGpu ? `${latestGpu.temp}°C` : '—' }}</span>
            <span v-if="latestGpu?.hotspot" class="text-xs font-medium text-rose-400">{{ tl('热点', 'HS') }} {{ latestGpu.hotspot }}°C</span>
            <span
              v-if="hotspotDelta(latestGpu) !== null"
              class="text-xs font-medium"
              :class="Number(hotspotDelta(latestGpu)) >= (settingsForm.hotspot_delta_warn || 20) ? 'text-rose-400 font-bold' : 'text-orange-400'"
            >Δ{{ hotspotDelta(latestGpu) }}°C</span>
            <span class="text-xs font-medium text-amber-400">{{ latestGpu ? `${latestGpu.power} W` : '' }}</span>
          </div>
          <div class="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-white/[0.06]">
            <div
              class="h-full rounded-full bg-gradient-to-r from-sky-400 via-amber-400 to-rose-500 transition-all duration-300"
              :style="{ width: `${Math.min(100, Number(latestGpu?.util || 0))}%` }"
            ></div>
          </div>
          <div v-if="state.stats?.gpu_temp" class="mt-1.5 font-mono text-[10.5px] text-slate-400">
            {{ tl(`极值 ${state.stats.gpu_temp.min}~${state.stats.gpu_temp.max}°C · 均${state.stats.gpu_temp.avg}°C`, `Range ${state.stats.gpu_temp.min}~${state.stats.gpu_temp.max}°C · Avg ${state.stats.gpu_temp.avg}°C`) }}
            <span v-if="state.stats?.gpu_power"> · {{ state.stats.gpu_power.max }}W {{ tl('峰', 'Peak') }}</span>
          </div>
        </div>

        <div class="relative overflow-hidden rounded-xl border border-white/[0.07] bg-[#121824]/90 p-3.5 transition hover:border-white/[0.14]">
          <div class="flex items-center justify-between text-xs text-slate-400">
            <span>{{ tl('核心频率 / 链路', 'Core Clock / Link') }}</span>
            <span class="rounded-lg bg-sky-500/10 p-1 text-sky-400"><Activity class="h-3.5 w-3.5" /></span>
          </div>
          <div class="mt-1.5 flex items-baseline gap-2 font-mono">
            <span class="text-lg font-bold text-white sm:text-xl">{{ latestGpu ? `${latestGpu.core} MHz` : '—' }}</span>
            <span class="text-xs font-medium" :class="latestGpu?.pcie_drop ? 'text-rose-400 font-bold' : 'text-sky-400'">
              {{ latestGpu?.pcie || '' }}{{ latestGpu?.pcie_drop ? tl(' (降速)', ' (Drop)') : '' }}
            </span>
          </div>
          <div class="mt-2 truncate font-mono text-[11px] text-slate-400">
            {{ tl('占用', 'Util') }} {{ latestGpu?.util ?? '—' }}% · {{ tl('显存', 'VRAM') }} {{ latestGpu?.vram ?? '—' }}{{ latestGpu?.vram_total ? `/${latestGpu.vram_total}` : '' }} MB
            <span v-if="latestGpu?.vram_pct > 0" :class="latestGpu.vram_pct >= (settingsForm.vram_warn_pct || 92) ? 'text-rose-400 font-bold' : 'text-sky-300'">({{ latestGpu.vram_pct }}%)</span>
          </div>
        </div>

        <div class="relative overflow-hidden rounded-xl border border-white/[0.07] bg-[#121824]/90 p-3.5 transition hover:border-white/[0.14]">
          <div class="flex items-center justify-between text-xs text-slate-400">
            <span>{{ tl('系统内存提交', 'System Memory Commit') }}</span>
            <span class="rounded-lg bg-emerald-500/10 p-1 text-emerald-400"><HardDrive class="h-3.5 w-3.5" /></span>
          </div>
          <div class="mt-1.5 font-mono text-[13px] font-bold tracking-tight text-white sm:text-lg">
            {{ trBackendText(latestMem.text) }}
          </div>
          <div class="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-white/[0.06]">
            <div
              class="h-full rounded-full transition-all duration-300"
              :class="latestMem.pct > 90 ? 'bg-rose-500' : 'bg-gradient-to-r from-emerald-500 to-teal-400'"
              :style="{ width: `${latestMem.pct}%` }"
            ></div>
          </div>
        </div>

        <div class="relative overflow-hidden rounded-xl border border-white/[0.07] bg-[#121824]/90 p-3.5 transition hover:border-white/[0.14]">
          <div class="flex items-center justify-between text-xs text-slate-400">
            <span>{{ tl('目标游戏进程', 'Target Game Process') }}</span>
            <span class="rounded-lg bg-amber-500/10 p-1 text-amber-400"><Gamepad2 class="h-3.5 w-3.5" /></span>
          </div>
          <div
            class="mt-1.5 truncate font-mono text-base font-bold sm:text-lg"
            :class="(state.proc_summary || '').includes('未运行') || !state.proc_summary ? 'text-slate-500' : 'text-emerald-400'"
          >
            {{ trBackendText(state.proc_summary) || '—' }}
          </div>
          <div class="mt-2 text-[11px] text-slate-500">
            {{ tl('开机自启：', 'Startup: ') }}
            <button @click="toggleStartup" class="underline decoration-dotted hover:text-slate-300">
              {{ state.startup_enabled ? tl('已开启', 'Enabled') : tl('未开启', 'Disabled') }}
            </button>
          </div>
        </div>
      </section>

      <!-- 操作工具栏与会话选择 -->
      <section class="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-white/[0.07] bg-[#121824]/90 px-3 py-2">
        <div class="flex flex-wrap items-center gap-2">
          <button
            @click="paused = !paused"
            class="inline-flex items-center gap-1.5 rounded-lg border border-white/[0.08] bg-white/[0.04] px-2.5 py-1.5 text-xs text-slate-200 transition hover:bg-white/[0.08]"
          >
            <Play v-if="paused" class="h-3.5 w-3.5 text-emerald-400" />
            <Pause v-else class="h-3.5 w-3.5 text-amber-400" />
            <span>{{ paused ? t.resume : t.pause }}</span>
          </button>

          <label class="flex cursor-pointer items-center gap-1.5 rounded-lg border border-white/[0.06] bg-white/[0.02] px-2.5 py-1.5 text-xs text-slate-300">
            <input type="checkbox" v-model="onlyBad" class="accent-sky-400" />
            <span>{{ t.onlyBad }}</span>
          </label>
        </div>

        <div class="flex flex-wrap items-center gap-2">
          <select
            :value="selectedSession"
            @change="onPickSession($event.target.value)"
            class="rounded-lg border border-white/[0.1] bg-[#0b0f17] px-2.5 py-1.5 font-mono text-xs text-slate-200 outline-none focus:border-sky-500"
          >
            <option v-if="!state.sessions.length" value="">{{ tl('(无会话)', '(No Sessions)') }}</option>
            <option v-for="s in state.sessions" :key="s.name" :value="s.name">
              {{ s.name === '__live__' ? tl('实时监测 (仅内存日志)', 'Live Monitor (Memory Only)') : s.name }} {{ s.active ? tl('(运行中)', '(Active)') : `(${trBackendText(s.size_label)})` }}
            </option>
          </select>

          <button
            @click="openBatchDelete"
            class="inline-flex items-center gap-1 rounded-lg border border-rose-500/20 bg-rose-500/10 px-2.5 py-1.5 text-xs text-rose-300 transition hover:bg-rose-500/20"
          >
            <Trash2 class="h-3.5 w-3.5" />
            <span>{{ tl('批量删除', 'Batch Delete') }}</span>
          </button>

          <button
            v-if="state.session && state.session !== '__live__' && !fullHistory"
            @click="loadFullHistory"
            class="rounded-lg border border-white/[0.08] bg-white/[0.04] px-2.5 py-1.5 text-xs text-slate-200 transition hover:bg-white/[0.08]"
          >
            {{ tl('加载全部', 'Load All') }}
          </button>

          <button
            v-if="state.session && state.session !== '__live__'"
            @click="exportZip"
            class="inline-flex items-center gap-1 rounded-lg border border-sky-500/30 bg-sky-500/15 px-2.5 py-1.5 text-xs text-sky-300 transition hover:bg-sky-500/25"
          >
            <Download class="h-3.5 w-3.5" />
            <span>{{ tl('打包导出', 'Export ZIP') }}</span>
          </button>

          <button
            @click="openFolder('session')"
            class="hidden items-center gap-1 rounded-lg border border-white/[0.08] bg-white/[0.04] px-2.5 py-1.5 text-xs text-slate-200 transition hover:bg-white/[0.08] sm:inline-flex"
          >
            <FolderOpen class="h-3.5 w-3.5" />
            <span>{{ tl('打开目录', 'Open Folder') }}</span>
          </button>
        </div>
      </section>

      <!-- 分段式胶囊导航栏（手机端可横向滑动） -->
      <nav class="flex gap-1.5 overflow-x-auto pb-0.5 [scrollbar-width:none] [-ms-overflow-style:none] [&::-webkit-scrollbar]:hidden">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          @click="activeTab = tab.id"
          class="inline-flex shrink-0 items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-semibold transition sm:px-3.5 sm:py-2 sm:text-sm"
          :class="
            activeTab === tab.id
              ? 'border-sky-500/40 bg-sky-500/15 text-white shadow-sm'
              : 'border-white/[0.06] bg-[#121824] text-slate-400 hover:text-slate-200'
          "
        >
          <component :is="tab.icon" class="h-4 w-4" />
          <span>{{ tab.label }}</span>
          <span
            v-if="tab.count > 0"
            class="rounded-full bg-white/[0.08] px-1.5 py-0.2 font-mono text-[10px] text-slate-300"
          >
            {{ tab.count }}
          </span>
        </button>
      </nav>

      <!-- 主内容区 -->
      <main class="flex-1 rounded-xl border border-white/[0.07] bg-[#121824]/90 p-3 sm:p-4">
        <div v-if="!state.session" class="flex h-64 items-center justify-center text-sm text-slate-500">
          {{ tl('暂无采集会话，请点击右上角「启动采集」开始后台记录', 'No capture session yet. Click "Start Capture" in the top-right to begin.') }}
        </div>

        <!-- 1) 实时 GPU -->
        <div v-else-if="activeTab === 'gpu'" class="flex flex-col gap-2.5">
          <div class="flex flex-wrap items-center justify-between gap-2 text-xs text-slate-400">
            <span>{{ tl(`已加载 ${state.gpu_rows.length} 条显卡样本${fullHistory ? '（完整历史）' : ''}`, `Loaded ${state.gpu_rows.length} GPU samples${fullHistory ? ' (Full History)' : ''}`) }}</span>
            <button
              @click="jumpToEnd"
              class="inline-flex items-center gap-1 rounded-lg border border-white/[0.08] bg-white/[0.04] px-2.5 py-1 text-xs text-slate-200 hover:bg-white/[0.08]"
            >
              <ArrowDownToLine class="h-3.5 w-3.5" />
              <span>{{ tl('跳到末尾（黑屏前）', 'Jump to Latest') }}</span>
            </button>
          </div>
          <div ref="gpuTableWrap" class="max-h-[60vh] overflow-auto rounded-lg border border-white/[0.07]">
            <table class="w-full min-w-[880px] border-collapse text-center text-xs">
              <thead class="sticky top-0 z-20 bg-[#171f2e]/95 text-slate-300 backdrop-blur">
                <tr>
                  <th class="sticky left-0 z-30 bg-[#171f2e] px-2 py-2.5">{{ tl('时间', 'Time') }}</th>
                  <th class="px-2 py-2.5">{{ tl('温度°C', 'Temp°C') }}</th>
                  <th class="px-2 py-2.5">{{ tl('热点°C', 'Hotspot°C') }}</th>
                  <th class="px-2 py-2.5">{{ tl('温差°C', 'Delta°C') }}</th>
                  <th class="px-2 py-2.5">{{ tl('功耗W', 'Power W') }}</th>
                  <th class="px-2 py-2.5">{{ tl('核心MHz', 'Core MHz') }}</th>
                  <th class="px-2 py-2.5">{{ tl('显存MHz', 'Mem MHz') }}</th>
                  <th class="px-2 py-2.5">PCIe</th>
                  <th class="px-2 py-2.5">{{ tl('占用%', 'Util%') }}</th>
                  <th class="px-2 py-2.5">{{ tl('显存MB', 'VRAM MB') }}</th>
                  <th class="px-2 py-2.5">{{ tl('风扇%', 'Fan%') }}</th>
                  <th class="px-2 py-2.5">{{ tl('降频原因', 'Throttle') }}</th>
                  <th class="px-2 py-2.5">{{ tl('状态', 'Status') }}</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-white/[0.05] font-mono">
                <tr v-for="(r, idx) in filteredGpuRows" :key="idx" :class="sevColor(r.sev)" class="odd:bg-white/[0.015] transition hover:bg-white/[0.04]">
                  <td class="sticky left-0 z-10 bg-[#121824] px-2 py-1.5">{{ r.ts }}</td>
                  <td class="px-2 py-1.5">{{ r.temp }}</td>
                  <td class="px-2 py-1.5">{{ r.hotspot || '—' }}</td>
                  <td class="px-2 py-1.5">{{ hotspotDelta(r) ?? '—' }}</td>
                  <td class="px-2 py-1.5">{{ r.power }}</td>
                  <td class="px-2 py-1.5">{{ r.core }}</td>
                  <td class="px-2 py-1.5">{{ r.memclk }}</td>
                  <td class="px-2 py-1.5" :class="r.pcie_drop ? 'text-rose-400 font-bold' : ''">{{ r.pcie }}{{ r.pcie_drop ? ' ⚠' : '' }}</td>
                  <td class="px-2 py-1.5">{{ r.util }}</td>
                  <td class="px-2 py-1.5">{{ r.vram_pct > 0 ? `${r.vram} (${r.vram_pct}%)` : r.vram }}</td>
                  <td class="px-2 py-1.5">{{ r.fan }}</td>
                  <td class="px-2 py-1.5" :title="r.thr || ''">{{ trThr(r.thr_label || r.thr) }}</td>
                  <td class="px-2 py-1.5">{{ trBackendText(r.ok) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>

        <!-- 1.5) CPU 实时监测（组件化、首屏重排、仅实时监测时展示） -->
        <CpuMonitorPanel
          v-else-if="activeTab === 'cpu' && state.cpu"
          :cpu="state.cpu"
          :cpu-points="state.cpu_points"
          :stats="state.stats"
          :live-running="state.live_running"
          :live-alert-banner="liveAlertBanner"
          :read-only-remote="state.read_only_remote"
          @dismiss-banner="liveAlertBanner = ''"
          @copy-summary="copyLiveSummary"
          @save-session="saveLiveSession"
        />

        <!-- 2) 趋势曲线 -->
        <div v-else-if="activeTab === 'trend'" class="flex flex-col gap-3">
          <div class="flex flex-wrap items-center gap-2 text-xs">
            <span class="text-slate-400">{{ tl('时间窗口：', 'Window:') }}</span>
            <div class="inline-flex rounded-lg border border-white/[0.08] bg-[#0b0f17] p-0.5">
              <button
                v-for="n in [15, 20, 30, 60]"
                :key="n"
                @click="trendWindow = n"
                class="rounded-md px-2.5 py-1 transition"
                :class="trendWindow === n ? 'bg-sky-500/20 text-sky-300 font-semibold' : 'text-slate-400 hover:text-slate-200'"
              >
                {{ n }} {{ tl('秒', 's') }}
              </button>
            </div>
            <button
              @click="jumpToEnd"
              class="inline-flex items-center gap-1 rounded-lg border border-white/[0.08] bg-white/[0.04] px-2.5 py-1 text-slate-200 hover:bg-white/[0.08]"
            >
              <ArrowDownToLine class="h-3.5 w-3.5" />
              <span>{{ tl('定位到末尾（黑屏前）', 'Jump to Latest') }}</span>
            </button>
            <span class="text-slate-500">{{ tl('点上方标数值 · 红色虚线为告警时刻', 'Hover for values · Red dashed line = Alert') }}</span>
          </div>

          <div class="grid grid-cols-1 gap-3 lg:grid-cols-2">
            <TrendCard
              v-for="c in trendCards"
              :key="c.name"
              :name="c.name"
              :unit="c.unit"
              :decimals="c.decimals"
              :color="c.color"
              :points="c.points"
              :alertTimes="alertTimes"
            />
          </div>
        </div>

        <!-- 3 & 4) 系统事件 / WER -->
        <div v-else-if="activeTab === 'events' || activeTab === 'wer'" class="flex flex-col gap-2.5">
          <div class="flex flex-wrap items-center gap-2 text-xs">
            <div class="relative w-full max-w-xs">
              <Search class="pointer-events-none absolute left-2.5 top-2 h-3.5 w-3.5 text-slate-500" />
              <input
                v-model="logFilter"
                :placeholder="tl('输入关键词（时间 / 来源 / 级别 / 消息）', 'Filter by keyword (time / source / level / message)')"
                class="w-full rounded-lg border border-white/[0.08] bg-[#0b0f17] py-1.5 pl-8 pr-7 text-xs text-slate-200 outline-none focus:border-sky-500"
              />
              <button v-if="logFilter" @click="logFilter = ''" class="absolute right-2 top-1.5 text-slate-500 hover:text-slate-200">
                <X class="h-3.5 w-3.5" />
              </button>
            </div>
            <span class="text-slate-400">
              {{ tl(`显示 ${(activeTab === 'events' ? filteredEvents : filteredWer).length} 条（点击行查看详情或复制）`, `Showing ${(activeTab === 'events' ? filteredEvents : filteredWer).length} entries (click row to inspect)`) }}
            </span>
          </div>

          <div class="max-h-[60vh] overflow-auto rounded-lg border border-white/[0.07]">
            <table class="w-full min-w-[680px] border-collapse text-left text-xs">
              <thead class="sticky top-0 z-10 bg-[#171f2e]/95 text-slate-300 backdrop-blur">
                <tr>
                  <th class="w-20 px-3 py-2.5">{{ tl('时间', 'Time') }}</th>
                  <th class="w-24 px-3 py-2.5">{{ tl('级别', 'Level') }}</th>
                  <th class="w-56 px-3 py-2.5">{{ activeTab === 'events' ? tl('来源 / 事件编号', 'Source / Event ID') : tl('创建时间', 'Created At') }}</th>
                  <th class="px-3 py-2.5">{{ tl('详情 / 消息', 'Details / Message') }}</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-white/[0.05]">
                <tr
                  v-for="(item, idx) in activeTab === 'events' ? filteredEvents : filteredWer"
                  :key="idx"
                  @click="logDetail = item"
                  class="cursor-pointer odd:bg-white/[0.015] transition hover:bg-white/[0.04]"
                >
                  <td class="px-3 py-2 font-mono text-slate-500">{{ item.time }}</td>
                  <td class="px-3 py-2">
                    <span class="inline-block rounded-md px-2 py-0.5 text-[11px] font-semibold" :class="badgeClass(item.sev)">
                      {{ item.level }}
                    </span>
                  </td>
                  <td class="max-w-[220px] truncate px-3 py-2 text-slate-300">{{ item.source }}</td>
                  <td class="max-w-[460px] truncate px-3 py-2 font-mono" :class="sevColor(item.sev)">
                    {{ item.message }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>

        <!-- 5) 告警与取证直览 -->
        <div v-else-if="activeTab === 'alerts'" class="flex flex-col gap-3">
          <div class="flex flex-wrap items-center gap-2 text-xs">
            <div class="relative w-full max-w-xs">
              <Search class="pointer-events-none absolute left-2.5 top-2 h-3.5 w-3.5 text-slate-500" />
              <input
                v-model="logFilter"
                :placeholder="tl('输入关键词筛选告警', 'Filter alerts by keyword')"
                class="w-full rounded-lg border border-white/[0.08] bg-[#0b0f17] py-1.5 pl-8 pr-3 text-xs text-slate-200 outline-none focus:border-sky-500"
              />
            </div>
            <span class="text-slate-400">{{ tl(`显示 ${filteredAlerts.length} 条`, `Showing ${filteredAlerts.length}`) }}</span>
          </div>

          <div class="max-h-56 overflow-auto rounded-lg border border-white/[0.07]">
            <table class="w-full min-w-[600px] border-collapse text-left text-xs">
              <thead class="sticky top-0 z-10 bg-[#171f2e]/95 text-slate-300 backdrop-blur">
                <tr>
                  <th class="w-20 px-3 py-2">{{ tl('时间', 'Time') }}</th>
                  <th class="w-24 px-3 py-2">{{ tl('级别', 'Level') }}</th>
                  <th class="w-28 px-3 py-2">{{ tl('来源', 'Source') }}</th>
                  <th class="px-3 py-2">{{ tl('告警详情', 'Alert Details') }}</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-white/[0.05]">
                <tr
                  v-for="(item, idx) in filteredAlerts"
                  :key="idx"
                  @click="logDetail = item"
                  class="cursor-pointer odd:bg-white/[0.015] transition hover:bg-white/[0.04]"
                >
                  <td class="px-3 py-1.5 font-mono text-slate-500">{{ item.time }}</td>
                  <td class="px-3 py-1.5">
                    <span class="inline-block rounded-md px-2 py-0.5 text-[11px] font-semibold" :class="badgeClass(item.sev)">
                      {{ item.level }}
                    </span>
                  </td>
                  <td class="px-3 py-1.5">{{ item.source }}</td>
                  <td class="px-3 py-1.5 font-mono" :class="sevColor(item.sev)">{{ item.message }}</td>
                </tr>
              </tbody>
            </table>
          </div>

          <div class="border-t border-white/[0.07] pt-3">
            <div class="mb-2 flex flex-wrap items-center gap-2 text-xs">
              <span class="text-sm font-semibold text-white">{{ tl(`事故取证直览（共 ${incidents.length} 条）`, `Incident Forensics (${incidents.length})`) }}</span>
              <button
                @click="fetchIncidents"
                class="inline-flex items-center gap-1 rounded-lg border border-white/[0.08] bg-white/[0.04] px-2.5 py-1 hover:bg-white/[0.08]"
              >
                <RefreshCw class="h-3 w-3" />
                <span>{{ tl('刷新', 'Refresh') }}</span>
              </button>
              <button
                v-for="fn in ['incident.json', 'Report_*.wer', 'wer-list.txt', 'nvidia-smi-q.txt']"
                :key="fn"
                @click="previewFile = fn; loadIncidentPreview()"
                class="rounded-lg border px-2.5 py-1 transition"
                :class="
                  previewFile === fn
                    ? 'border-sky-500/40 bg-sky-500/20 text-sky-300'
                    : 'border-white/[0.06] bg-white/[0.03] text-slate-300 hover:bg-white/[0.07]'
                "
              >
                {{ fn }}
              </button>
            </div>

            <div class="grid grid-cols-1 gap-3 md:grid-cols-12">
              <div class="max-h-56 overflow-auto rounded-lg border border-white/[0.07] bg-[#0b0f17] p-2 md:col-span-4">
                <div v-if="!incidents.length" class="py-6 text-center text-xs text-slate-500">{{ tl('暂无事故取证记录', 'No incident records') }}</div>
                <button
                  v-for="inc in incidents"
                  :key="inc.name"
                  @click="selectedIncident = inc.name; loadIncidentPreview()"
                  class="mb-1 block w-full rounded-lg px-2.5 py-1.5 text-left text-xs transition"
                  :class="selectedIncident === inc.name ? 'bg-sky-500/20 text-white ring-1 ring-sky-500/40' : 'hover:bg-white/[0.04]'"
                >
                  <span v-if="inc.belongs_to_session" class="mr-1 text-emerald-400">{{ tl('[本次]', '[Cur]') }}</span>
                  <span class="font-mono">{{ inc.name }}</span>
                  <span class="ml-1 text-slate-400">{{ inc.reason }}</span>
                </button>
              </div>
              <pre class="max-h-56 overflow-auto whitespace-pre-wrap rounded-lg border border-white/[0.07] bg-[#0b0f17] p-3 font-mono text-xs text-slate-200 md:col-span-8">{{ previewText || tl('(暂无预览内容)', '(No preview)') }}</pre>
            </div>
          </div>
        </div>
      </main>
    </div>

    <!-- 手机扫码弹窗 -->
    <div v-if="showQr" class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-3 backdrop-blur-sm">
      <div class="w-full max-w-xs rounded-2xl border border-white/[0.1] bg-[#121824] p-5 text-center shadow-2xl">
        <div class="mb-3 flex items-center justify-between">
          <div class="flex items-center gap-2 text-sm font-semibold text-white">
            <Smartphone class="h-4 w-4 text-sky-400" />
            <span>{{ tl('手机扫码访问', 'Scan QR to Connect') }}</span>
          </div>
          <button @click="showQr = false" class="rounded-lg p-1 text-slate-400 hover:bg-white/[0.06] hover:text-white">
            <X class="h-4 w-4" />
          </button>
        </div>
        <div class="mx-auto mb-3 inline-block rounded-xl border border-white/[0.08] bg-[#0b0f17] p-2">
          <img :src="qrDataUrl" alt="QR Code" class="h-48 w-48" />
        </div>
        <div v-if="state.ipv6_url" class="mb-2 text-[11px] text-slate-400">
          {{ tl('二维码已同时包含局域网 IPv4 与外网 IPv6，使用 GPUCap 安卓端扫码可选择连接方式', 'QR contains both LAN IPv4 and public IPv6. Scan with GPUCap Android to choose.') }}
        </div>
        <div class="mb-3 break-all rounded-lg bg-[#0b0f17] px-2.5 py-1.5 font-mono text-xs text-sky-400">
          {{ state.lan_url }}
        </div>
        <div v-if="state.ipv6_url" class="mb-3 break-all rounded-lg bg-[#0b0f17] px-2.5 py-1.5 font-mono text-[11px] text-emerald-400">
          {{ tl('外网 IPv6：', 'Public IPv6: ') }}{{ state.ipv6_url }}
        </div>
        <div class="flex flex-col gap-2">
          <button
            @click="copyText(state.lan_url, tl('已复制局域网访问地址', 'LAN URL copied'))"
            class="inline-flex w-full items-center justify-center gap-1.5 rounded-lg bg-sky-600 py-2 text-xs font-semibold text-white hover:bg-sky-500"
          >
            <Copy class="h-3.5 w-3.5" />
            <span>{{ tl('复制局域网地址', 'Copy LAN URL') }}</span>
          </button>
          <button
            v-if="state.ipv6_url"
            @click="copyText(state.ipv6_url, tl('已复制外网 IPv6 地址', 'Public IPv6 URL copied'))"
            class="inline-flex w-full items-center justify-center gap-1.5 rounded-lg bg-emerald-600 py-2 text-xs font-semibold text-white hover:bg-emerald-500"
          >
            <Copy class="h-3.5 w-3.5" />
            <span>{{ tl('复制外网 IPv6 地址', 'Copy Public IPv6 URL') }}</span>
          </button>
        </div>
      </div>
    </div>

    <!-- 日志详情弹窗（完整展示 + 复制） -->
    <div v-if="logDetail" class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-3 backdrop-blur-sm">
      <div class="w-full max-w-lg rounded-2xl border border-white/[0.1] bg-[#121824] p-5 shadow-2xl">
        <div class="mb-3 flex items-center justify-between">
          <div class="flex items-center gap-2">
            <span class="rounded-md px-2 py-0.5 text-xs font-semibold" :class="badgeClass(logDetail.sev)">{{ logDetail.level }}</span>
            <span class="font-mono text-xs text-slate-400">{{ logDetail.time }}</span>
          </div>
          <button @click="logDetail = null" class="rounded-lg p-1 text-slate-400 hover:bg-white/[0.06] hover:text-white">
            <X class="h-4 w-4" />
          </button>
        </div>
        <div class="mb-2 text-xs text-slate-400">{{ tl('来源：', 'Source: ') }}{{ logDetail.source }}</div>
        <pre class="max-h-64 overflow-auto whitespace-pre-wrap break-all rounded-lg border border-white/[0.08] bg-[#0b0f17] p-3 font-mono text-xs text-slate-200">{{ logDetail.message }}</pre>
        <div class="mt-4 flex justify-end gap-2">
          <button @click="logDetail = null" class="rounded-lg border border-white/[0.08] bg-white/[0.04] px-3 py-1.5 text-xs">{{ tl('关闭', 'Close') }}</button>
          <button @click="copyText(logDetail.raw, tl('已复制完整原始记录', 'Full raw entry copied'))" class="inline-flex items-center gap-1 rounded-lg bg-sky-600 px-3.5 py-1.5 text-xs font-semibold text-white hover:bg-sky-500">
            <Copy class="h-3.5 w-3.5" />
            <span>{{ tl('复制整条', 'Copy Full') }}</span>
          </button>
        </div>
      </div>
    </div>

    <!-- 设置弹窗 -->
    <div v-if="showSettings" class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-3 backdrop-blur-sm">
      <div class="w-full max-w-lg rounded-2xl border border-white/[0.1] bg-[#121824] p-5 shadow-2xl">
        <div class="mb-4 flex items-center justify-between">
          <h3 class="text-base font-semibold text-white">{{ tl('采集与界面设置', 'Capture & UI Settings') }}</h3>
          <button @click="showSettings = false" class="rounded-lg p-1 text-slate-400 hover:bg-white/[0.06] hover:text-white">
            <X class="h-4 w-4" />
          </button>
        </div>
        <div class="grid grid-cols-1 gap-3 text-xs sm:grid-cols-2">
          <label class="flex flex-col gap-1">
            <span class="text-slate-400">{{ tl('目标监控显卡', 'Target GPU') }}</span>
            <select v-if="gpuList.length" v-model.number="settingsForm.gpu_index" class="rounded-lg border border-white/[0.1] bg-[#0b0f17] px-2.5 py-2 outline-none focus:border-sky-500">
              <option v-for="g in gpuList" :key="g.index" :value="g.index">#{{ g.index }} {{ g.name }}</option>
            </select>
            <input v-else type="number" v-model.number="settingsForm.gpu_index" class="rounded-lg border border-white/[0.1] bg-[#0b0f17] px-2.5 py-2 outline-none focus:border-sky-500" />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-slate-400">{{ tl('监控游戏进程（逗号分隔）', 'Watched Games (comma-separated)') }}</span>
            <input v-model="settingsForm.game_names_text" class="rounded-lg border border-white/[0.1] bg-[#0b0f17] px-2.5 py-2 outline-none focus:border-sky-500" />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-slate-400">{{ tl('温度告警阈值 (°C)', 'GPU Temp Warn (°C)') }}</span>
            <input type="number" v-model.number="settingsForm.temp_warn" class="rounded-lg border border-white/[0.1] bg-[#0b0f17] px-2.5 py-2 outline-none focus:border-sky-500" />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-slate-400">{{ tl('热点告警阈值 (°C)', 'Hotspot Warn (°C)') }}</span>
            <input type="number" v-model.number="settingsForm.hotspot_warn" class="rounded-lg border border-white/[0.1] bg-[#0b0f17] px-2.5 py-2 outline-none focus:border-sky-500" />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-slate-400">{{ tl('温差告警阈值 (°C)', 'Delta Warn (°C)') }}</span>
            <input type="number" v-model.number="settingsForm.hotspot_delta_warn" class="rounded-lg border border-white/[0.1] bg-[#0b0f17] px-2.5 py-2 outline-none focus:border-sky-500" />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-slate-400">{{ tl('功耗告警阈值 (W)', 'Power Warn (W)') }}</span>
            <input type="number" v-model.number="settingsForm.power_warn" class="rounded-lg border border-white/[0.1] bg-[#0b0f17] px-2.5 py-2 outline-none focus:border-sky-500" />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-slate-400">{{ tl('CPU 温度告警阈值 (°C)', 'CPU Temp Warn (°C)') }}</span>
            <input type="number" v-model.number="settingsForm.cpu_temp_warn" class="rounded-lg border border-white/[0.1] bg-[#0b0f17] px-2.5 py-2 outline-none focus:border-sky-500" />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-slate-400">{{ tl('CPU 占用告警阈值 (%)', 'CPU Usage Warn (%)') }}</span>
            <input type="number" v-model.number="settingsForm.cpu_usage_warn" class="rounded-lg border border-white/[0.1] bg-[#0b0f17] px-2.5 py-2 outline-none focus:border-sky-500" />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-slate-400">{{ tl('显存占用告警阈值 (%)', 'VRAM Usage Warn (%)') }}</span>
            <input type="number" v-model.number="settingsForm.vram_warn_pct" class="rounded-lg border border-white/[0.1] bg-[#0b0f17] px-2.5 py-2 outline-none focus:border-sky-500" />
          </label>
          <label class="flex items-center gap-2 sm:col-span-2">
            <input type="checkbox" v-model="settingsForm.pcie_drop_warn" class="accent-sky-500" />
            <span class="text-slate-300">{{ tl('开启高负载下 PCIe 带宽降级预警', 'Alert on PCIe link downgrade under load') }}</span>
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-slate-400">{{ tl('GPU 采样间隔 (秒)', 'GPU Interval (s)') }}</span>
            <input type="number" step="0.5" v-model.number="settingsForm.gpu_interval" class="rounded-lg border border-white/[0.1] bg-[#0b0f17] px-2.5 py-2 outline-none focus:border-sky-500" />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-slate-400">{{ tl('会话切分时长 (小时，0 不切分)', 'Session Split Hours (0 = off)') }}</span>
            <input type="number" v-model.number="settingsForm.max_session_hours" class="rounded-lg border border-white/[0.1] bg-[#0b0f17] px-2.5 py-2 outline-none focus:border-sky-500" />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-slate-400">{{ tl('远程访问口令（留空不限制）', 'Remote PIN (blank = off)') }}</span>
            <input type="password" v-model="settingsForm.auth_pin" class="rounded-lg border border-white/[0.1] bg-[#0b0f17] px-2.5 py-2 outline-none focus:border-sky-500" />
          </label>
          <label class="flex items-center gap-2 sm:col-span-2">
            <input type="checkbox" v-model="settingsForm.read_only_remote" class="accent-sky-500" />
            <span class="text-slate-300">{{ tl('开启远程只读访客模式（禁止远程控制启停与改配置）', 'Read-only remote guest mode (disable remote controls)') }}</span>
          </label>
        </div>
        <div class="mt-5 flex justify-end gap-2">
          <button @click="showSettings = false" class="rounded-lg border border-white/[0.08] bg-white/[0.04] px-3.5 py-2 text-xs hover:bg-white/[0.08]">{{ tl('取消', 'Cancel') }}</button>
          <button @click="saveSettings" class="rounded-lg bg-emerald-600 px-4 py-2 text-xs font-semibold text-white hover:bg-emerald-500">{{ tl('保存设置', 'Save Settings') }}</button>
        </div>
      </div>
    </div>

    <!-- 访问口令解锁弹窗 -->
    <div v-if="showPinModal" class="fixed inset-0 z-50 flex items-center justify-center bg-black/80 p-3 backdrop-blur-sm">
      <div class="w-full max-w-sm rounded-2xl border border-white/[0.1] bg-[#121824] p-5 shadow-2xl">
        <h3 class="text-base font-bold text-white">{{ t.pinTitle }}</h3>
        <p class="mt-1 text-xs text-slate-400">{{ t.pinDesc }}</p>
        <input
          v-model="pinInput"
          type="password"
          :placeholder="t.pinPlaceholder"
          @keyup.enter="verifyPin"
          class="mt-3 w-full rounded-lg border border-white/[0.12] bg-[#0b0f17] px-3 py-2 text-sm text-white outline-none focus:border-sky-500"
        />
        <p v-if="pinError" class="mt-1.5 text-xs text-rose-400">{{ pinError }}</p>
        <div class="mt-4 flex justify-end">
          <button @click="verifyPin" class="rounded-lg bg-sky-600 px-4 py-2 text-xs font-semibold text-white hover:bg-sky-500">{{ t.unlock }}</button>
        </div>
      </div>
    </div>

    <!-- 首次启动语言选择弹窗 -->
    <div v-if="showLangPicker" class="fixed inset-0 z-50 flex items-center justify-center bg-black/85 p-3 backdrop-blur-sm">
      <div class="w-full max-w-sm rounded-2xl border border-white/[0.12] bg-[#121824] p-6 text-center shadow-2xl">
        <h3 class="text-base font-bold text-white">Choose Language / 请选择语言</h3>
        <p class="mt-1.5 text-xs text-slate-400">Select your preferred interface language (changeable anytime).</p>
        <div class="mt-5 grid grid-cols-2 gap-3">
          <button
            @click="setAppLang('en')"
            class="rounded-xl border border-sky-500/40 bg-sky-500/15 py-2.5 text-sm font-bold text-sky-300 transition hover:bg-sky-500/25"
          >
            English
          </button>
          <button
            @click="setAppLang('zh-CN')"
            class="rounded-xl border border-emerald-500/40 bg-emerald-500/15 py-2.5 text-sm font-bold text-emerald-300 transition hover:bg-emerald-500/25"
          >
            简体中文
          </button>
        </div>
      </div>
    </div>

    <!-- 批量删除弹窗 -->
    <div v-if="showBatchDelete" class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-3 backdrop-blur-sm">
      <div class="w-full max-w-md rounded-2xl border border-white/[0.1] bg-[#121824] p-5 shadow-2xl">
        <div class="mb-3 flex items-center justify-between">
          <h3 class="text-base font-semibold text-white">{{ tl('批量删除会话', 'Batch Delete Sessions') }}</h3>
          <button @click="showBatchDelete = false" class="rounded-lg p-1 text-slate-400 hover:bg-white/[0.06] hover:text-white">
            <X class="h-4 w-4" />
          </button>
        </div>
        <div class="mb-2.5 flex flex-wrap gap-1.5 text-xs">
          <button @click="batchSelectAll" class="rounded-md bg-white/[0.06] px-2.5 py-1 hover:bg-white/[0.1]">{{ tl('全选', 'All') }}</button>
          <button @click="batchSelected = new Set()" class="rounded-md bg-white/[0.06] px-2.5 py-1 hover:bg-white/[0.1]">{{ tl('取消全选', 'None') }}</button>
          <button @click="batchSelectOthers" class="rounded-md bg-white/[0.06] px-2.5 py-1 hover:bg-white/[0.1]">{{ tl('仅选非当前', 'Others') }}</button>
          <button @click="batchInvert" class="rounded-md bg-white/[0.06] px-2.5 py-1 hover:bg-white/[0.1]">{{ tl('反选', 'Invert') }}</button>
        </div>
        <div class="max-h-64 space-y-1 overflow-auto rounded-lg border border-white/[0.07] bg-[#0b0f17] p-2 text-xs">
          <label
            v-for="s in state.sessions"
            :key="s.name"
            class="flex items-center justify-between rounded-md px-2.5 py-1.5 hover:bg-white/[0.04]"
          >
            <div class="flex items-center gap-2">
              <input
                type="checkbox"
                :disabled="s.active"
                :checked="!s.active && batchSelected.has(s.name)"
                @change="toggleBatchItem(s.name)"
                class="accent-sky-400"
              />
              <span class="font-mono">{{ s.name }}</span>
              <span v-if="s.active" class="text-emerald-400">{{ tl('[采集中]', '[Active]') }}</span>
              <span v-else-if="s.name === state.session" class="text-sky-400">{{ tl('[当前查看]', '[Current]') }}</span>
            </div>
            <span class="font-mono text-slate-500">{{ s.size_label }}</span>
          </label>
        </div>
        <div class="mt-4 flex items-center justify-between text-xs">
          <span class="text-slate-400">{{ tl(`已选中 ${batchSelected.size} 个会话`, `${batchSelected.size} selected`) }}</span>
          <div class="flex gap-2">
            <button @click="showBatchDelete = false" class="rounded-lg border border-white/[0.08] bg-white/[0.04] px-3 py-1.5 hover:bg-white/[0.08]">{{ tl('关闭', 'Close') }}</button>
            <button
              :disabled="!batchSelected.size"
              @click="confirmBatchDelete"
              class="rounded-lg bg-rose-600 px-3.5 py-1.5 font-semibold text-white hover:bg-rose-500 disabled:opacity-40"
            >
              {{ tl(`删除所选 (${batchSelected.size})`, `Delete (${batchSelected.size})`) }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
