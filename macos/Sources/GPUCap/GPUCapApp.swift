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
