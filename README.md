# GPUCap

> **Language / 语言**: **English** | [简体中文](README.zh-CN.md)

**A lightweight Windows hardware background collector and cross-platform real-time monitoring dashboard built for diagnosing black screens, GPU drops, driver crashes, and system instability.**

`GPUCap` prioritizes usability and low overhead, offering a **Windows native desktop client (`egui`)**, an **embedded web server (`Axum` + `Vue 3`)**, **native Android/tablet clients (`Jetpack Compose` & `Rust + UniFFI`)**, and a **macOS Apple Silicon client (`SwiftUI` + `WKWebView`)**. Run collection locally on Windows and turn any tablet, phone, or browser into a live secondary telemetry display over LAN or IPv6.

---

## Key Features

### 1. Mutually Exclusive Modes: Persistent Logging & In-Memory Live Monitor
* **Background Collector (Disk Logging)**: Continuously records `gpu.csv`, `mem.csv`, Windows Event Logs (`WHEA-Logger`, `nvlddmkm`, `Display`, `Kernel-Power`, `BugCheck`, etc.), and `WER` crash dumps with automatic rotation and one-click `.zip` export.
* **In-Memory Live Monitor**: Zero disk writes and minimal overhead for gaming or stress testing. Clicking **Stop Live** keeps the last snapshot in memory for review, with one-click **Copy Diagnostic Summary (Markdown)** or **Save as Full Session**.

### 2. Full-Stack GPU & CPU Telemetry
* **GPU Sensors & Throttle Reason Decoding**: Collects core/hotspot temperatures, delta, power draw, core/memory clocks, fan speed, VRAM usage, and PCIe link status via `NVML` (with `nvidia-smi` fallback) and `NVAPI`, automatically decoding `clocks_throttle_reasons` bitmasks into readable labels (`Power Cap`, `Thermal Cap`, `SW Thermal`, `HW Slowdown`, `Sync Boost`, `Idle`).
* **CPU Ring Gauges & Hybrid P/E-Core Topology**: Automatically detects P-Cores and E-Cores, rendering total usage, temperature, average/peak clocks, and per-core ring gauges inside a dedicated tab (hidden when Live Monitor is inactive).
* **60s Trend Chart & Top 5 Processes**: Plots 60-second CPU usage/temperature trends with red alert markers alongside Top 5 CPU and Top 5 Memory processes.
* **Real-Time Extremes (`Min / Avg / Max`)**: Tracks minimum, average, and peak values for GPU temperature, hotspot, power, and CPU load.

### 3. Hardware Alerts & Environment Fingerprint
* **Targeted Alerts**: Configurable thresholds for GPU temp, hotspot, hotspot delta, power, CPU temp/usage, **VRAM usage (%)**, and **PCIe bandwidth degradation under load**, displaying live alert banners across all platforms.
* **Environment Fingerprint & Driver Change Detection**: Captures GPU model, driver version, total VRAM, max PCIe capability, VBIOS, Windows version, and active power plan on start, automatically logging driver version changes.

### 4. Multi-Device Companion Experience & Security
* **Desktop Always-on-Top HUD**: Collapses into a compact `720×52` floating overlay during full-screen gaming or benchmarks.
* **Android Tablet Big HUD, Full-Screen Tabs & Keep-Screen-On**: Supports long-distance large-font HUD mode, persistent `FLAG_KEEP_SCREEN_ON`, compact split-pane navigation, and a full-screen paginated view with a "Jump to Latest" button.
* **Bi-Directional Auto-Follow Sync**: Starting Live Monitor or Background Collector on Windows automatically unlocks stale sessions and switches connected web/Android clients to the active stream.
* **LAN Auto-Discovery & Access PIN**: Built-in UDP `8788` discovery (`GPUCAP_DISCOVER`), QR code pairing, optional `X-GPUCap-Pin` authentication, and read-only guest mode.
* **Bilingual UI (English / 简体中文)**: First-run language picker across all platforms with runtime switching in Settings.

---

## Project Structure

```text
gpucap/
├── src/                  # Rust backend, collectors, Windows desktop GUI (egui) & web server (axum)
│   ├── main.rs           # Entry point & CLI subcommands
│   ├── collectors.rs     # NVML / NVAPI / Memory / Process / Event Log / WER collectors
│   ├── cpu_probe.rs      # CPU temp, hybrid P/E-core topology & Top 5 processes
│   ├── live.rs           # In-memory live monitor engine, stats & snapshot export
│   ├── web.rs            # REST API, PIN auth middleware & UDP 8788 discovery responder
│   ├── ui.rs             # Windows native desktop GUI & floating HUD
│   └── infra.rs          # Config, throttle decoder, environment fingerprint & helpers
├── web/                  # Vue 3 + Vite + TailwindCSS web frontend (embedded via rust-embed)
├── android-native/       # Kotlin + Jetpack Compose native Android/tablet client
├── android-rust/         # Rust core (UniFFI + Tokio + Reqwest) + Compose Android client
│   └── rust-core/        # Rust async HTTP, UDP discovery, PIN auth & typed models
├── android/              # Legacy WebView Android wrapper client
└── macos/                # SwiftUI + WKWebView macOS (Apple Silicon) client
```

---

## Quick Start & Building from Source

### Prerequisites
* **Rust** (`1.80+`) & **Node.js** (`18+`)
* **NVIDIA GPU Driver** (`NVML` supported; host collection runs on Windows)
* **Android NDK `28.2` + Gradle `9.6.1` + JDK 17** (for Android builds)

### 1. Web Frontend & Windows Desktop / Server
```powershell
npm --prefix .\web install
npm --prefix .\web run build
cargo test --locked
cargo build --release --locked
```

### 2. Native Android Client (`android-native`)
```powershell
gradle -p .\android-native assembleRelease
adb install -r .\android-native\app\build\outputs\apk\release\app-release.apk
```

### 3. Rust + Compose Android Client (`android-rust`)
```powershell
cd .\android-rust\rust-core
cargo build --target aarch64-linux-android --release
Copy-Item -Force .\target\aarch64-linux-android\release\libgpucap_core.so ..\app\src\main\jniLibs\arm64-v8a\libgpucap_core.so
cargo run --bin uniffi-bindgen -- generate --library .\target\aarch64-linux-android\release\libgpucap_core.so --language kotlin --out-dir ..\app\src\main\java

cd ..\..
gradle -p .\android-rust assembleRelease
adb install -r .\android-rust\app\build\outputs\apk\release\app-release.apk
```

### 4. macOS Client (Apple Silicon)
See [macos/README.md](macos/README.md):
```bash
cd macos && chmod +x build_arm64.sh && ./build_arm64.sh
```

---

## CLI Usage

```text
gpucap.exe [COMMAND]

Commands:
  run                Run collector in foreground (--minutes <N>)
  daemon             Run collector silently in background
  stop               Stop background collector
  status             Check background collector status
  supervise          Watchdog mode (auto-restart on crash)
  ui                 Launch desktop UI (--session / --minimized / --selftest)
  install-startup    Register Windows startup task
  uninstall-startup  Remove Windows startup task
```

---

## Ports & GitHub Actions Release

| Port | Protocol | Purpose |
| :--- | :--- | :--- |
| `8787` | HTTP (TCP IPv4/IPv6) | Embedded Web UI & REST API |
| `8788` | UDP | LAN host discovery (`GPUCAP_DISCOVER`) |

Push a version tag (`git tag v0.3.0 && git push origin v0.3.0`) or run **Actions → Release Build** manually to build and publish `gpucap-setup-x64.exe`, `gpucap.exe`, `gpucap-android-native.apk`, `gpucap-android-rust.apk`, and `GPUCap-macos-arm64.zip`.

To build the Windows x64 installer locally (requires [Inno Setup 6](https://jrsoftware.org/isinfo.php)):
```powershell
powershell -ExecutionPolicy Bypass -File .\installer\build_installer.ps1
```
