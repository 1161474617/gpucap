import Foundation
import SwiftUI

/// 统一深色配色规范，与 Windows / Web / Android 端保持一致
enum Theme {
    static let bg = Color(hex: 0x0B0F17)
    static let topBar = Color(hex: 0x0F141E)
    static let card = Color(hex: 0x121A28)
    static let inputBg = Color(hex: 0x0D131F)
    static let border = Color(hex: 0x233044)
    static let text = Color(hex: 0xE2E8F0)
    static let dim = Color(hex: 0x8492A6)
    static let accent = Color(hex: 0x38BDF8)
    static let emerald = Color(hex: 0x10B981)
    static let emeraldText = Color(hex: 0x06281E)
    static let accentText = Color(hex: 0x071926)
    static let btnSurface = Color(hex: 0x1E293B)
    static let bad = Color(hex: 0xEF4444)
}

extension Color {
    init(hex: UInt32, opacity: Double = 1.0) {
        let r = Double((hex >> 16) & 0xFF) / 255.0
        let g = Double((hex >> 8) & 0xFF) / 255.0
        let b = Double(hex & 0xFF) / 255.0
        self.init(.sRGB, red: r, green: g, blue: b, opacity: opacity)
    }
}

/// 连接状态指示点状态
enum ConnectionStatus {
    case connecting
    case connected
    case failed
}

/// 全局应用状态：负责地址规范化、历史记录持久化、页面切换及提示通知
@MainActor
final class AppState: ObservableObject {
    private static let keyLastUrl = "gpucap_last_url"
    private static let keyHistory = "gpucap_history_urls"
    static let defaultPort = 8787

    @Published var isShowingWeb: Bool = false
    @Published var currentUrl: String = ""
    @Published var inputUrl: String = ""
    @Published var historyUrls: [String] = []
    @Published var connectionStatus: ConnectionStatus = .connecting
    @Published var loadProgress: Double = 0
    @Published var errorMessage: String? = nil
    @Published var toastMessage: String? = nil
    @Published var reloadTrigger: UUID = UUID()

    private var toastTask: Task<Void, Never>?

    init() {
        historyUrls = loadHistory()
        let saved = UserDefaults.standard.string(forKey: Self.keyLastUrl) ?? ""
        inputUrl = saved
        if !saved.isEmpty {
            openWebUrl(saved)
        }
    }

    /// 规范化并校验用户输入或二维码识别到的地址（提取内嵌 URL、校验主机、补全默认端口）
    func normalizeUrl(_ raw: String) -> String? {
        var s = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        if s.isEmpty { return nil }

        // 容错提取多段文本中的 http(s) 地址
        if let regex = try? NSRegularExpression(pattern: #"https?://[^\s"'<>]+"#, options: .caseInsensitive) {
            let range = NSRange(s.startIndex..., in: s)
            if let match = regex.firstMatch(in: s, range: range),
               let swiftRange = Range(match.range, in: s) {
                s = String(s[swiftRange])
            }
        }

        if !s.lowercased().hasPrefix("http://") && !s.lowercased().hasPrefix("https://") {
            s = "http://" + s
        }
        while s.hasSuffix("/") {
            s.removeLast()
        }

        guard let comps = URLComponents(string: s),
              let host = comps.host,
              !host.isEmpty else {
            return nil
        }

        let hostAllowed = CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.-:[]")
        if host.rangeOfCharacter(from: hostAllowed.inverted) != nil {
            return nil
        }

        if comps.port == nil && comps.path.isEmpty {
            s += ":\(Self.defaultPort)"
        }
        return s
    }

    /// 保存地址并进入监控视图
    func saveAndOpen(_ url: String) {
        UserDefaults.standard.set(url, forKey: Self.keyLastUrl)
        addHistory(url)
        inputUrl = url
        openWebUrl(url)
    }

    /// 切换到 Web 监控页并加载目标地址
    func openWebUrl(_ url: String) {
        currentUrl = url
        errorMessage = nil
        connectionStatus = .connecting
        loadProgress = 0.08
        isShowingWeb = true
        reloadTrigger = UUID()
    }

    /// 刷新当前监控页
    func reloadWeb() {
        errorMessage = nil
        connectionStatus = .connecting
        loadProgress = 0.08
        reloadTrigger = UUID()
    }

    /// 返回连接配置页
    func showConnectPage() {
        isShowingWeb = false
        historyUrls = loadHistory()
    }

    func showToast(_ message: String) {
        toastTask?.cancel()
        toastMessage = message
        toastTask = Task {
            try? await Task.sleep(nanoseconds: 2_600_000_000)
            if !Task.isCancelled {
                toastMessage = nil
            }
        }
    }

    // MARK: - 历史记录持久化

    private func loadHistory() -> [String] {
        UserDefaults.standard.stringArray(forKey: Self.keyHistory) ?? []
    }

    private func addHistory(_ url: String) {
        var list = loadHistory()
        list.removeAll { $0 == url }
        list.insert(url, at: 0)
        if list.count > 8 {
            list = Array(list.prefix(8))
        }
        UserDefaults.standard.set(list, forKey: Self.keyHistory)
        historyUrls = list
    }

    func removeHistory(_ url: String) {
        var list = loadHistory()
        list.removeAll { $0 == url }
        UserDefaults.standard.set(list, forKey: Self.keyHistory)
        historyUrls = list
    }
}
