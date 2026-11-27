# GPUCap

**面向黑屏、掉卡、驱动崩溃与稳定性排障的 Windows 硬件长期后台采集器与跨端实时监控平台。**

`GPUCap` 以轻量化与实用性为核心，提供 **Windows 原生桌面端（`egui`）**、**内置网页服务端（`Axum` + `Vue 3`）**、**纯原生安卓/平板客户端（`Jetpack Compose`）** 与 **macOS 客户端（`SwiftUI` + `WKWebView`）**，支持在电脑本机运行采集与诊断，并通过局域网或 IPv6 将平板、手机变为实时副屏仪表盘。

---

## 核心特性

### 1. 双模式互斥：后台落盘取证 & 纯内存实时监测
* **后台采集模式（落盘取证）**：持续记录 `gpu.csv`、`mem.csv`、系统事件日志（`WHEA-Logger`、`nvlddmkm`、`Display`、`Kernel-Power`、`BugCheck` 等）及 `WER` 崩溃报告，支持按时长/大小切分会话与一键打包 `.zip` 导出。
* **纯内存实时监测模式**：零磁盘写入、低开销运行，适合日常玩游戏或烤机盯盘；点击「停止监测」后保留内存中的最后一次采样日志供当前继续查看，并支持**一键复制 Markdown 诊断摘要**或**手动补存为正式会话**。

### 2. 全维度 GPU / CPU 监控与专业解码
* **GPU 核心传感器与降频中文解码**：基于原生 `NVML`（支持 `nvidia-smi` 回退）与 `NVAPI` 采集核心温度、热点温度（Hotspot）及温差、实时功耗、核心/显存频率、风扇转速、显存占用与 `PCIe` 链路速率，并将 `clocks_throttle_reasons` 掩码自动解码为 `功耗墙`、`温度墙`、`软件温控`、`硬件降频`、`同步限频` 等中文标签。
* **CPU 环形仪表盘与大小核感知**：自动识别混合架构（P 性能核 / E 能效核），提供总占用率、温度、均频/峰值及逐逻辑核心环形进度条；仅在开启「实时监测」时展示于独立分栏，不占用主界面空间。
* **60 秒趋势与 Top 5 进程榜**：内置 CPU 60 秒实时占用/温度折线与异常红点标注，同步列出 Top 5 CPU 与 Top 5 内存占用进程，快速定位高负载来源。
* **实时极值统计（`Min / Avg / Max`）**：自动汇总当前会话 GPU 温度、热点、功耗与 CPU 占用/温度的最小、平均与峰值指标。

### 3. 专项异常预警与系统环境指纹
* **专项硬件预警**：支持自定义 GPU 温度、热点、温差、功耗、CPU 温度/占用、**显存（VRAM）占用率**阈值，以及**高负载下 PCIe 带宽降级（掉速）预警**；触发告警时全平台顶部弹出高亮提醒横幅。
* **环境指纹与驱动变更记录**：启动实时监测时自动采集显卡型号、驱动版本、总显存、`PCIe` 硬件上限、`VBIOS`、Windows 版本与当前电源计划，并在检测到驱动版本变更时主动写入告警记录与诊断摘要。

### 4. 跨端副屏体验与访问安全
* **桌面「置顶浮条模式」**：桌面端支持一键切换为极简置顶小窗，全屏游戏/压测时悬浮显示关键 GPU/CPU 指标与告警数。
* **平板「大字仪表盘」、「全屏分页」与「副屏常亮」**：原生安卓端提供远距离大字仪表盘视图与持久化屏幕常亮控制（`FLAG_KEEP_SCREEN_ON`），平板横屏支持双栏紧凑导航；全屏分页内置采样极值统计条、「回到最新」悬浮按钮及实时/常亮快捷开关。
* **三端双向自动跟随同步**：桌面端、网页端与安卓端自动跟踪主机最新活跃状态；Windows 端点击「实时监测」或「启动采集」后，移动端与网页端会自动解除旧会话锁定并秒级切入最新数据流。
* **局域网自动发现与扫码直连**：服务端默认监听 `8787`（HTTP/IPv4/IPv6）与 `8788`（UDP 局域网广播发现），原生安卓端支持自动扫描在线主机与相机扫码连接。
* **访问口令与只读访客模式**：可在设置中配置远程访问口令（`X-GPUCap-Pin`）及只读模式（禁止远程修改设置或启停采集）。

---

## 项目结构

```text
gpucap/
├── src/                  # Rust 核心后端、采集引擎、桌面 GUI (egui) 与内置 Web 服务 (axum)
│   ├── main.rs           # 统一入口与 CLI 子命令解析
│   ├── collectors.rs     # NVML / NVAPI / 内存 / 进程 / 事件日志 / WER 采集器
│   ├── cpu_probe.rs      # CPU 温度、混合架构 P/E 核拓扑与 Top 进程探针
│   ├── live.rs           # 纯内存实时监测引擎、极值汇总与快照补存
│   ├── web.rs            # REST API、鉴权中间件与 UDP 8788 局域网发现应答器
│   ├── ui.rs             # Windows 原生桌面端 GUI 与置顶浮条视图
│   └── infra.rs          # 配置管理、降频掩码解码、环境指纹与系统辅助函数
├── web/                  # Vue 3 + Vite + TailwindCSS 网页前端（内嵌至 Rust 可执行文件）
├── android-native/       # Kotlin + Jetpack Compose 纯原生安卓/平板客户端
├── android-rust/         # Rust 核心引擎 (UniFFI + Tokio + Reqwest) + Compose 混合架构安卓端
│   └── rust-core/        # Rust 异步网络、UDP 发现、PIN 鉴权与数据模型核心库
└── macos/                # SwiftUI + WKWebView macOS (Apple Silicon) 客户端
```

---

## 快速开始与源码构建

### 前置依赖
* **Rust 工具链**（`1.80+`）与 **Node.js**（`18+`，用于构建内嵌网页前端）
* **NVIDIA 显卡驱动**（支持 `NVML`；桌面端采集仅支持 Windows 平台）
* **Android Studio / Gradle + JDK 17**（仅构建原生安卓端时需要）

### 1. 构建网页端与 Windows 桌面/服务端
> 内置网页资源会在编译 Rust 时通过 `rust-embed` 自动打包进 `gpucap.exe`，因此需先构建前端再编译 Rust。

```powershell
# 1. 安装前端依赖并构建 dist 产物
npm --prefix .\web install
npm --prefix .\web run build

# 2. 运行单元测试
cargo test

# 3. 编译 Release 可执行文件（产物位于 target\release\gpucap.exe）
cargo build --release
```

### 2. 构建纯原生安卓/平板客户端（`android-native`）

```powershell
# 编译 Release APK（产物位于 android-native\app\build\outputs\apk\release\app-release.apk）
gradle -p .\android-native assembleRelease

# 通过 ADB 安装到已连接的安卓设备/平板
adb install -r .\android-native\app\build\outputs\apk\release\app-release.apk
```

### 3. 构建 Rust 核心 + Compose 安卓客户端（`android-rust`）
> 采用 `UniFFI 0.29` + `Tokio` + `Reqwest (rustls-tls)` 驱动全部网络、UDP 发现与 JSON 解析，需要安装 `aarch64-linux-android` Rust 目标与 Android NDK `28.2`。

```powershell
# 1. 进入 Rust 核心目录编译 arm64 Release 动态库并生成 UniFFI Kotlin 绑定
cd .\android-rust\rust-core
cargo build --target aarch64-linux-android --release
Copy-Item -Force .\target\aarch64-linux-android\release\libgpucap_core.so ..\app\src\main\jniLibs\arm64-v8a\libgpucap_core.so
cargo run --bin uniffi-bindgen -- generate --library .\target\aarch64-linux-android\release\libgpucap_core.so --language kotlin --out-dir ..\app\src\main\java

# 2. 编译 Release APK 并安装到设备
cd ..\..
gradle -p .\android-rust assembleRelease
adb install -r .\android-rust\app\build\outputs\apk\release\app-release.apk
```

### 4. 构建 macOS 客户端（需在 Apple Silicon Mac 上执行）

详细构建步骤请参考 [macos/README.md](macos/README.md)：

```bash
cd macos
chmod +x build_arm64.sh
./build_arm64.sh
```

---

## 命令行用法

直接双击运行 `gpucap.exe` 将启动图形监控界面；也支持以下子命令用于无人值守采集与自动化调度：

```text
gpucap.exe [COMMAND]

Commands:
  run                前台运行采集（支持 --minutes <N> 指定运行时长）
  daemon             后台静默运行采集（无窗口）
  stop               停止后台采集任务
  status             查看当前后台采集状态
  supervise          守护进程模式（采集异常崩溃时自动重启）
  ui                 打开实时监控桌面界面（支持 --session / --minimized / --selftest）
  install-startup    注册 Windows 开机自启动
  uninstall-startup  移除 Windows 开机自启动
```

---

## 远程连接与端口说明

| 端口 | 协议 | 用途 |
| :--- | :--- | :--- |
| `8787` | HTTP (TCP IPv4/IPv6) | 网页控制台及原生客户端 REST API 端口（可在设置中自定义） |
| `8788` | UDP | 局域网主机自动发现广播端口（响应 `GPUCAP_DISCOVER` 查询） |

* 若需在局域网或公网 IPv6 环境下使用，建议在「设置」中开启**远程访问口令**，或按需启用**远程只读访客模式**以防止外部设备误操作主机采集状态。

---

## GitHub Actions 自动构建正式版

仓库内置了多平台自动发布工作流 [release.yml](.github/workflows/release.yml)，支持并行构建并打包发布全平台正式版产物（`gpucap.exe`、`gpucap-android-native.apk`、`gpucap-android-rust.apk` 与 `GPUCap-macos-arm64.zip`）：

```bash
git tag v0.3.0
git push origin v0.3.0
```

也可以在 GitHub 仓库的 **Actions → Release Build → Run workflow** 中手动触发正式版构建。

