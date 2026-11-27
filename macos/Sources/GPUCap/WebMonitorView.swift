import AppKit
import SwiftUI
import WebKit

/// 内嵌网页监控页面：含顶部胶囊工具栏、加载进度条、WKWebView 容器与断线提示卡片
struct WebMonitorView: View {
    @EnvironmentObject private var state: AppState

    var body: some View {
        VStack(spacing: 0) {
            // 顶部胶囊控制栏
            HStack(spacing: 8) {
                HStack(spacing: 8) {
                    Circle()
                        .fill(dotColor)
                        .frame(width: 7, height: 7)

                    Text(state.currentUrl)
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundColor(Theme.text)
                        .lineLimit(1)
                        .truncationMode(.middle)

                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .background(Color(hex: 0x162032))
                .overlay(
                    RoundedRectangle(cornerRadius: 99)
                        .stroke(Theme.border, lineWidth: 1)
                )
                .cornerRadius(99)

                Button {
                    state.setLanguage(state.isZh ? .en : .zhCn)
                } label: {
                    Text(state.isZh ? "EN" : "中文")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundColor(Theme.accent)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(Color(hex: 0x16263A))
                        .cornerRadius(9)
                }
                .buttonStyle(.plain)

                Button {
                    state.reloadWeb()
                } label: {
                    Text(state.tr("刷新", "Reload"))
                        .font(.system(size: 12, weight: .medium))
                        .foregroundColor(Theme.text)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .background(Theme.btnSurface)
                        .cornerRadius(9)
                }
                .buttonStyle(.plain)

                Button {
                    state.showConnectPage()
                } label: {
                    Text(state.tr("切换主机", "Switch Host"))
                        .font(.system(size: 12, weight: .medium))
                        .foregroundColor(Theme.accent)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .background(Color(hex: 0x16263A))
                        .overlay(
                            RoundedRectangle(cornerRadius: 9)
                                .stroke(Color(hex: 0x264363), lineWidth: 1)
                        )
                        .cornerRadius(9)
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(Theme.topBar)

            // 顶部细进度条
            if state.loadProgress > 0 && state.loadProgress < 1.0 {
                GeometryReader { geo in
                    Rectangle()
                        .fill(Theme.accent)
                        .frame(width: geo.size.width * state.loadProgress, height: 2)
                }
                .frame(height: 2)
                .background(Theme.topBar)
            }

            // 主内容区
            ZStack {
                Theme.bg.ignoresSafeArea()

                WebViewContainer(
                    urlString: state.currentUrl,
                    reloadTrigger: state.reloadTrigger,
                    state: state
                )

                if let errMsg = state.errorMessage {
                    ZStack {
                        Theme.bg
                        CardBox {
                            VStack(spacing: 14) {
                                Text(state.tr("无法连接到 GPUCap 服务", "Unable to Connect to GPUCap"))
                                    .font(.system(size: 17, weight: .bold))
                                    .foregroundColor(Color(hex: 0xFCA5A5))

                                Text(errMsg)
                                    .font(.system(size: 13))
                                    .foregroundColor(Theme.dim)
                                    .multilineTextAlignment(.center)

                                Button {
                                    state.openWebUrl(state.currentUrl)
                                } label: {
                                    Text(state.tr("重新连接", "Reconnect"))
                                        .font(.system(size: 14, weight: .bold))
                                        .foregroundColor(Theme.emeraldText)
                                        .frame(maxWidth: .infinity)
                                        .padding(.vertical, 10)
                                        .background(Theme.emerald)
                                        .cornerRadius(10)
                                }
                                .buttonStyle(.plain)

                                Button {
                                    state.showConnectPage()
                                } label: {
                                    Text(state.tr("返回选择主机", "Back to Host Selection"))
                                        .font(.system(size: 14, weight: .bold))
                                        .foregroundColor(Theme.accent)
                                        .frame(maxWidth: .infinity)
                                        .padding(.vertical, 10)
                                        .overlay(
                                            RoundedRectangle(cornerRadius: 10)
                                                .stroke(Theme.accent, lineWidth: 1)
                                        )
                                }
                                .buttonStyle(.plain)
                            }
                        }
                        .frame(maxWidth: 400)
                        .padding(24)
                    }
                }
            }
        }
        .background(Theme.bg)
    }

    private var dotColor: Color {
        switch state.connectionStatus {
        case .connecting: return Theme.accent
        case .connected: return Theme.emerald
        case .failed: return Theme.bad
        }
    }
}

/// 封装 macOS 原生 WKWebView，支持加载进度、错误拦截及 .zip 取证包下载接管
struct WebViewContainer: NSViewRepresentable {
    let urlString: String
    let reloadTrigger: UUID
    let state: AppState

    func makeCoordinator() -> Coordinator {
        Coordinator(state: state)
    }

    func makeNSView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .nonPersistent()
        // 同步将 macOS 已选择的语言写入网页端 localStorage，避免重复弹出语言选择框
        let langJs = "if (!localStorage.getItem('gpucap_lang')) { localStorage.setItem('gpucap_lang', '\(state.lang.rawValue)'); }"
        config.userContentController.addUserScript(
            WKUserScript(source: langJs, injectionTime: .atDocumentStart, forMainFrameOnly: true)
        )

        let webView = WKWebView(frame: .zero, configuration: config)
        webView.navigationDelegate = context.coordinator
        webView.setValue(false, forKey: "drawsBackground")

        context.coordinator.observeProgress(webView)
        context.coordinator.load(urlString: urlString, on: webView, forceReload: true)
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        if context.coordinator.lastUrl != urlString || context.coordinator.lastTrigger != reloadTrigger {
            context.coordinator.load(urlString: urlString, on: webView, forceReload: true)
        }
    }

    final class Coordinator: NSObject, WKNavigationDelegate, WKDownloadDelegate {
        let state: AppState
        var lastUrl: String = ""
        var lastTrigger: UUID = UUID()
        private var progressObs: NSKeyValueObservation?
        private var pendingDownloadName: String = ""

        init(state: AppState) {
            self.state = state
        }

        func observeProgress(_ webView: WKWebView) {
            progressObs = webView.observe(\.estimatedProgress, options: [.new]) { [weak self] view, _ in
                Task { @MainActor in
                    self?.state.loadProgress = view.estimatedProgress
                }
            }
        }

        func load(urlString: String, on webView: WKWebView, forceReload: Bool) {
            lastUrl = urlString
            lastTrigger = state.reloadTrigger
            guard let url = URL(string: urlString) else { return }
            if webView.url == url && forceReload {
                webView.reload()
            } else {
                var req = URLRequest(url: url)
                req.cachePolicy = .reloadIgnoringLocalCacheData
                webView.load(req)
            }
        }

        // MARK: - WKNavigationDelegate

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            Task { @MainActor in
                state.loadProgress = 1.0
                if state.errorMessage == nil {
                    state.connectionStatus = .connected
                }
            }
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
            handleLoadError(error)
        }

        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
            handleLoadError(error)
        }

        private func handleLoadError(_ error: Error) {
            let nsErr = error as NSError
            if nsErr.domain == NSURLErrorDomain && nsErr.code == NSURLErrorCancelled {
                return
            }
            Task { @MainActor in
                state.loadProgress = 1.0
                state.connectionStatus = .failed
                state.errorMessage = state.tr(
                    "无法连接到 \(state.currentUrl)\n请检查 Windows 端是否已开启「网页服务」，且 Mac 与电脑处于同一局域网。",
                    "Could not connect to \(state.currentUrl).\nMake sure the Windows host web service is running and both devices are on the same LAN."
                )
            }
        }

        // 拦截不可直接渲染的文件响应（如打包导出的 .zip），转为下载任务
        func webView(
            _ webView: WKWebView,
            decidePolicyFor navigationResponse: WKNavigationResponse,
            decisionHandler: @escaping (WKNavigationResponsePolicy) -> Void
        ) {
            if !navigationResponse.canShowMIMEType {
                decisionHandler(.download)
            } else {
                decisionHandler(.allow)
            }
        }

        func webView(_ webView: WKWebView, navigationResponse: WKNavigationResponse, didBecome download: WKDownload) {
            download.delegate = self
        }

        // MARK: - WKDownloadDelegate

        func download(
            _ download: WKDownload,
            decideDestinationUsing response: URLResponse,
            suggestedFilename: String,
            completionHandler: @escaping (URL?) -> Void
        ) {
            let downloadsDir = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first
                ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Downloads")
            var dest = downloadsDir.appendingPathComponent(suggestedFilename)
            if FileManager.default.fileExists(atPath: dest.path) {
                let stem = dest.deletingPathExtension().lastPathComponent
                let ext = dest.pathExtension
                let ts = Int(Date().timeIntervalSince1970)
                let altName = ext.isEmpty ? "\(stem)-\(ts)" : "\(stem)-\(ts).\(ext)"
                dest = downloadsDir.appendingPathComponent(altName)
            }
            pendingDownloadName = dest.lastPathComponent
            completionHandler(dest)
        }

        func downloadDidFinish(_ download: WKDownload) {
            let name = pendingDownloadName
            Task { @MainActor in
                state.showToast(state.tr("已保存到「下载」：\(name)", "Saved to Downloads: \(name)"))
            }
        }

        func download(_ download: WKDownload, didFailWithError error: Error, resumeData: Data?) {
            Task { @MainActor in
                state.showToast(state.tr("下载失败：\(error.localizedDescription)", "Download failed: \(error.localizedDescription)"))
            }
        }
    }
}
