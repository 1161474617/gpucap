import { computed, ref } from 'vue'

export const LANG_STORAGE_KEY = 'gpucap_lang'

const savedLang = typeof window !== 'undefined' ? localStorage.getItem(LANG_STORAGE_KEY) || '' : ''

export const currentLang = ref(savedLang === 'en' ? 'en' : 'zh-CN')
export const showLangPicker = ref(!savedLang)

export function setAppLang(lang) {
  const next = lang === 'en' ? 'en' : 'zh-CN'
  currentLang.value = next
  showLangPicker.value = false
  if (typeof window !== 'undefined') {
    localStorage.setItem(LANG_STORAGE_KEY, next)
  }
}

const dict = {
  en: {
    appSub: 'Hardware Stability & Live Telemetry',
    startCollect: 'Collect',
    stopCollect: 'Stop',
    startLive: 'Live',
    stopLive: 'Stop Live',
    webQr: 'Pair / QR',
    settings: 'Settings',
    gpuCardTitle: 'GPU Temp / Power',
    clockCardTitle: 'Core Clock / Link',
    memCardTitle: 'RAM Commit',
    procCardTitle: 'Target Process',
    autoStart: 'Auto Start',
    enabled: 'Enabled',
    disabled: 'Disabled',
    tabGpu: 'Live GPU',
    tabCpu: 'CPU Monitor',
    tabTrend: 'Trends',
    tabEvents: 'Events',
    tabWer: 'WER / Crashes',
    tabAlerts: 'Alerts & Dumps',
    pause: 'Pause',
    resume: 'Resume',
    onlyBad: 'Alerts Only',
    loadAll: 'Full History',
    exportZip: 'Export ZIP',
    openDir: 'Open Folder',
    copySummary: 'Copy Summary',
    saveSession: 'Save Session',
    liveRunningDesc: 'Live monitoring (in-memory)',
    liveStoppedDesc: 'Live stopped (snapshot kept)',
    cpuTrendTitle: 'CPU 60s Trend & Anomaly Markers',
    topCpuProcs: 'Top 5 CPU Processes',
    topMemProcs: 'Top 5 Memory Processes',
    expandCores: 'Show Per-Core Rings',
    collapseCores: 'Hide Per-Core Rings',
    settingsTitle: 'Collector & Display Settings',
    languageLabel: 'Interface Language',
    cancel: 'Cancel',
    save: 'Save',
    pinTitle: 'Host Access PIN Required',
    pinDesc: 'Please enter the access PIN configured on the host PC.',
    pinPlaceholder: 'Enter PIN',
    unlock: 'Unlock',
  },
  'zh-CN': {
    appSub: '硬件稳定性与实时监控平台',
    startCollect: '启动采集',
    stopCollect: '停止采集',
    startLive: '实时监测',
    stopLive: '停止监测',
    webQr: '扫码连接',
    settings: '设置',
    gpuCardTitle: '显卡温度 / 功耗',
    clockCardTitle: '核心频率 / 链路',
    memCardTitle: '系统内存提交',
    procCardTitle: '目标游戏进程',
    autoStart: '开机自启',
    enabled: '已开启',
    disabled: '未开启',
    tabGpu: '实时 GPU',
    tabCpu: 'CPU 监测',
    tabTrend: '趋势曲线',
    tabEvents: '系统事件',
    tabWer: 'WER / 事故',
    tabAlerts: '告警与取证',
    pause: '暂停',
    resume: '继续',
    onlyBad: '只看异常',
    loadAll: '全部历史',
    exportZip: '打包导出',
    openDir: '打开目录',
    copySummary: '复制诊断摘要',
    saveSession: '补存为完整会话',
    liveRunningDesc: '实时监测中（纯内存缓冲）',
    liveStoppedDesc: '已停止监测（保留本次快照）',
    cpuTrendTitle: 'CPU 60 秒实时趋势与异常点标注',
    topCpuProcs: 'Top 5 CPU 占用进程',
    topMemProcs: 'Top 5 内存占用进程',
    expandCores: '展开逐逻辑核心环形负载',
    collapseCores: '收起逐逻辑核心环形负载',
    settingsTitle: '采集与界面设置',
    languageLabel: '界面语言',
    cancel: '取消',
    save: '保存设置',
    pinTitle: '请输入主机访问口令',
    pinDesc: '电脑端已开启访问保护，验证口令后方可读取数据。',
    pinPlaceholder: '输入访问口令',
    unlock: '验证解锁',
  },
}

export const t = computed(() => dict[currentLang.value] || dict['zh-CN'])

export function tl(zh, en) {
  return currentLang.value === 'zh-CN' ? zh : en
}

export function trThr(raw) {
  if (!raw || raw === '—' || currentLang.value !== 'en') return raw || '—'
  return raw
    .replaceAll('功耗墙', 'PowerCap')
    .replaceAll('温度墙', 'ThermalCap')
    .replaceAll('软件温控', 'SWThermal')
    .replaceAll('硬件降频', 'HWSlowdown')
    .replaceAll('同步限频', 'SyncBoost')
    .replaceAll('显示限制', 'DisplayCap')
    .replaceAll('应用限频', 'AppClock')
    .replaceAll('空闲', 'Idle')
}

export function trBackendText(raw) {
  if (!raw || currentLang.value !== 'en') return raw || ''
  return raw
    .replace('● 实时监测中（仅内存日志）', '● Live Monitor (Memory Only)')
    .replace('● 实时监测已停止（保留内存日志）', '● Live Monitor Stopped')
    .replace('● 未在采集（暂无会话）', '● Idle (No Session)')
    .replace('● 未在采集（查看历史）', '● Idle (Viewing History)')
    .replace('● 采集器运行中', '● Collector Running')
    .replace('● 采集器初始化中...', '● Initializing...')
    .replace('● 心跳延迟 ', '● Heartbeat Lag ')
    .replace('● 无心跳（采集器未运行）', '● No Heartbeat (Stopped)')
    .replace(/^内存\s*/, 'RAM ')
    .replace('可用 ', 'Avail ')
    .replace('非分页 ', 'NonPaged ')
    .replace(/^游戏\s*/, 'Game ')
    .replace('未运行', 'Not Running')
    .replace('仅内存', 'Memory')
    .replace('正常', 'OK')
    .replace('异常', 'Alert')
}
