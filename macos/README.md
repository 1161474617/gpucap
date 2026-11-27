# GPUCap macOS (Apple Silicon arm64) 客户端

基于原生 `SwiftUI` + `WKWebView` 构建，与安卓端保持一致的深色视觉规范。

## 功能特性
- **手动输入连接**：支持输入 `IP` 或完整网址（无端口时默认补全 `8787`），自动校验并保存历史。
- **历史连接记录**：基于 `UserDefaults` 本地持久化，支持点击直连与移除。
- **监控视图 (`WKWebView`)**：顶部胶囊工具栏（连接状态指示点、地址显示、刷新、切换主机）、加载进度条、断线重连引导，并接管网页端 `.zip` 导出下载至 `~/Downloads`。

## 构建方法（需在 Apple Silicon Mac 上执行）

要求：`macOS 13.0+`，已安装 `Xcode` 或 `Command Line Tools`（`xcode-select --install`）。

```bash
cd macos
chmod +x build_arm64.sh
./build_arm64.sh
```

构建完成后将生成：
- `macos/dist/GPUCap.app`
- `macos/dist/GPUCap-macos-arm64.zip`
