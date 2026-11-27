import SwiftUI

/// 连接配置首页：包含手动输入连接、独立扫码/识图卡片及历史连接记录
struct ConnectView: View {
    @EnvironmentObject private var state: AppState

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                // 顶部品牌标题区
                HStack(spacing: 12) {
                    Text("GPU")
                        .font(.system(size: 12, weight: .bold, design: .monospaced))
                        .foregroundColor(Theme.accent)
                        .frame(width: 44, height: 44)
                        .background(Color(hex: 0x16263A))
                        .overlay(
                            RoundedRectangle(cornerRadius: 12)
                                .stroke(Theme.accent, lineWidth: 1)
                        )
                        .cornerRadius(12)

                    VStack(alignment: .leading, spacing: 3) {
                        Text(state.tr("GPUCap 无线监控", "GPUCap Wireless Monitor"))
                            .font(.system(size: 20, weight: .bold))
                            .foregroundColor(.white)
                        Text(state.tr(
                            "与 Windows 主机处于同一局域网并开启网页服务即可实时监控",
                            "Connect over LAN with the Windows host web service enabled"
                        ))
                            .font(.system(size: 12))
                            .foregroundColor(Theme.dim)
                    }
                    Spacer()

                    Button {
                        state.setLanguage(state.isZh ? .en : .zhCn)
                    } label: {
                        Text(state.isZh ? "EN" : "中文")
                            .font(.system(size: 12, weight: .semibold))
                            .foregroundColor(Theme.accent)
                            .padding(.horizontal, 10)
                            .padding(.vertical, 6)
                            .background(Color(hex: 0x16263A))
                            .overlay(
                                RoundedRectangle(cornerRadius: 8)
                                    .stroke(Color(hex: 0x264363), lineWidth: 1)
                            )
                            .cornerRadius(8)
                    }
                    .buttonStyle(.plain)
                }
                .padding(.bottom, 4)

                // 1. 手动输入连接主机卡片
                CardBox {
                    VStack(alignment: .leading, spacing: 12) {
                        Text(state.tr("连接电脑端主机", "Connect to Host"))
                            .font(.system(size: 15, weight: .bold))
                            .foregroundColor(.white)

                        HStack(spacing: 8) {
                            TextField(
                                state.tr("输入 IP 或网址，例如 192.168.1.100:8787", "Enter IP or URL, e.g. 192.168.1.100:8787"),
                                text: $state.inputUrl
                            )
                                .textFieldStyle(.plain)
                                .font(.system(size: 14, design: .monospaced))
                                .foregroundColor(.white)
                                .onSubmit { connectManual() }

                            if !state.inputUrl.isEmpty {
                                Button {
                                    state.inputUrl = ""
                                } label: {
                                    Image(systemName: "xmark.circle.fill")
                                        .foregroundColor(Theme.dim)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                        .background(Theme.inputBg)
                        .overlay(
                            RoundedRectangle(cornerRadius: 10)
                                .stroke(Theme.border, lineWidth: 1)
                        )
                        .cornerRadius(10)

                        Button(action: connectManual) {
                            Text(state.tr("保存并连接", "Save & Connect"))
                                .font(.system(size: 14, weight: .bold))
                                .foregroundColor(Theme.emeraldText)
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 11)
                                .background(Theme.emerald)
                                .cornerRadius(10)
                        }
                        .buttonStyle(.plain)
                    }
                }

                // 2. 历史连接记录卡片
                CardBox {
                    VStack(alignment: .leading, spacing: 10) {
                        Text(state.tr("历史连接记录", "Recent Hosts"))
                            .font(.system(size: 15, weight: .bold))
                            .foregroundColor(.white)

                        if state.historyUrls.isEmpty {
                            Text(state.tr("暂无历史记录", "No recent hosts"))
                                .font(.system(size: 12))
                                .foregroundColor(Theme.dim)
                        } else {
                            VStack(spacing: 8) {
                                ForEach(state.historyUrls, id: \.self) { url in
                                    HStack(spacing: 8) {
                                        Button {
                                            state.saveAndOpen(url)
                                        } label: {
                                            HStack {
                                                Text(url)
                                                    .font(.system(size: 12.5, design: .monospaced))
                                                    .foregroundColor(Theme.text)
                                                    .lineLimit(1)
                                                    .truncationMode(.middle)
                                                Spacer()
                                            }
                                            .padding(.horizontal, 12)
                                            .padding(.vertical, 9)
                                            .background(Theme.inputBg)
                                            .overlay(
                                                RoundedRectangle(cornerRadius: 9)
                                                    .stroke(Theme.border, lineWidth: 1)
                                            )
                                            .cornerRadius(9)
                                        }
                                        .buttonStyle(.plain)

                                        Button {
                                            state.removeHistory(url)
                                        } label: {
                                            Text(state.tr("移除", "Remove"))
                                                .font(.system(size: 12))
                                                .foregroundColor(Color(hex: 0xFCA5A5))
                                                .padding(.horizontal, 10)
                                                .padding(.vertical, 9)
                                                .background(Color(hex: 0x2A161B))
                                                .overlay(
                                                    RoundedRectangle(cornerRadius: 9)
                                                        .stroke(Color(hex: 0x5B212A), lineWidth: 1)
                                                )
                                                .cornerRadius(9)
                                        }
                                        .buttonStyle(.plain)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            .frame(maxWidth: 560)
            .padding(.horizontal, 20)
            .padding(.vertical, 24)
            .frame(maxWidth: .infinity)
        }
        .background(Theme.bg)
    }

    private func connectManual() {
        guard let norm = state.normalizeUrl(state.inputUrl) else {
            state.showToast(state.tr("请输入有效的 IP 或网址", "Please enter a valid IP or URL"))
            return
        }
        state.saveAndOpen(norm)
    }
}

/// 统一深色圆角卡片容器
struct CardBox<Content: View>: View {
    @ViewBuilder let content: () -> Content

    var body: some View {
        content()
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.card)
            .overlay(
                RoundedRectangle(cornerRadius: 16)
                    .stroke(Theme.border, lineWidth: 1)
            )
            .cornerRadius(16)
    }
}
