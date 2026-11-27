import AppKit
import SwiftUI

/// GPUCap macOS 客户端应用入口
@main
struct GPUCapApp: App {
    @StateObject private var state = AppState()

    init() {
        NSApplication.shared.setActivationPolicy(.regular)
        NSApplication.shared.appearance = NSAppearance(named: .darkAqua)
    }

    var body: some Scene {
        WindowGroup("GPUCap") {
            ZStack(alignment: .bottom) {
                Group {
                    if state.isShowingWeb {
                        WebMonitorView()
                    } else {
                        ConnectView()
                    }
                }
                .environmentObject(state)

                if state.showLangPicker {
                    Color.black.opacity(0.65)
                        .ignoresSafeArea()

                    VStack(spacing: 14) {
                        Text("Choose Language / 请选择语言")
                            .font(.system(size: 18, weight: .bold))
                            .foregroundColor(.white)

                        Text("Select your preferred display language. You can change this anytime.\n请选择默认显示语言，之后可随时在界面右上角切换。")
                            .font(.system(size: 12.5))
                            .foregroundColor(Theme.dim)
                            .multilineTextAlignment(.center)
                            .lineSpacing(4)

                        HStack(spacing: 12) {
                            Button {
                                state.setLanguage(.en)
                            } label: {
                                Text("English")
                                    .font(.system(size: 14, weight: .bold))
                                    .foregroundColor(Theme.accentText)
                                    .frame(maxWidth: .infinity)
                                    .padding(.vertical, 11)
                                    .background(Theme.accent)
                                    .cornerRadius(10)
                            }
                            .buttonStyle(.plain)

                            Button {
                                state.setLanguage(.zhCn)
                            } label: {
                                Text("简体中文")
                                    .font(.system(size: 14, weight: .bold))
                                    .foregroundColor(Theme.emeraldText)
                                    .frame(maxWidth: .infinity)
                                    .padding(.vertical, 11)
                                    .background(Theme.emerald)
                                    .cornerRadius(10)
                            }
                            .buttonStyle(.plain)
                        }
                        .padding(.top, 4)
                    }
                    .padding(24)
                    .frame(maxWidth: 420)
                    .background(Theme.card)
                    .overlay(
                        RoundedRectangle(cornerRadius: 16)
                            .stroke(Theme.border, lineWidth: 1)
                    )
                    .cornerRadius(16)
                    .shadow(color: .black.opacity(0.5), radius: 20, y: 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }

                if let msg = state.toastMessage {
                    Text(msg)
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundColor(Theme.text)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 9)
                        .background(Theme.btnSurface)
                        .overlay(
                            RoundedRectangle(cornerRadius: 10)
                                .stroke(Theme.border, lineWidth: 1)
                        )
                        .cornerRadius(10)
                        .shadow(color: .black.opacity(0.4), radius: 10, y: 4)
                        .padding(.bottom, 20)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .frame(minWidth: 440, minHeight: 620)
            .preferredColorScheme(.dark)
            .onAppear {
                NSApplication.shared.activate(ignoringOtherApps: true)
            }
        }
        .defaultSize(width: 1120, height: 760)
    }
}
