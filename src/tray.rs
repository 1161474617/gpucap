//! 系统托盘管理模块：生成纯内存图标，并通过实时事件回调唤醒隐藏中的界面或可靠退出。

use crate::infra::set_main_window_visible;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Clone, Copy)]
pub enum TrayAction {
    ShowWindow,
    HideWindow,
    StartCollector,
    StopCollector,
    QuitUiOnly,
    QuitAll,
}

pub struct TrayManager {
    _tray: TrayIcon,
    rx: Receiver<TrayAction>,
    pub allow_quit: Arc<AtomicBool>,
}

impl TrayManager {
    pub fn new(ctx: egui::Context, on_stop_all: fn()) -> Result<Self, String> {
        let icon = build_rgba_icon()?;
        let menu = Menu::new();
        let item_show = MenuItem::new("显示主界面", true, None);
        let item_hide = MenuItem::new("隐藏到托盘", true, None);
        let item_start = MenuItem::new("启动后台采集", true, None);
        let item_stop = MenuItem::new("停止后台采集", true, None);
        let item_quit_ui = MenuItem::new("退出界面（保持后台采集）", true, None);
        let item_quit_all = MenuItem::new("停止采集并全部退出", true, None);

        let id_show = item_show.id().0.clone();
        let id_hide = item_hide.id().0.clone();
        let id_start = item_start.id().0.clone();
        let id_stop = item_stop.id().0.clone();
        let id_quit_ui = item_quit_ui.id().0.clone();
        let id_quit_all = item_quit_all.id().0.clone();

        let _ = menu.append(&item_show);
        let _ = menu.append(&item_hide);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&item_start);
        let _ = menu.append(&item_stop);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&item_quit_ui);
        let _ = menu.append(&item_quit_all);

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("GPUCap — 显卡与系统异常取证监控")
            .with_icon(icon)
            .build()
            .map_err(|e| format!("创建托盘图标失败：{}", e))?;

        let (tx, rx) = mpsc::channel::<TrayAction>();
        let allow_quit = Arc::new(AtomicBool::new(false));

        let tx_menu = tx.clone();
        let ctx_menu = ctx.clone();
        let quit_flag_menu = Arc::clone(&allow_quit);
        MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
            let id = &ev.id.0;
            if id == &id_show {
                set_main_window_visible(true);
                ctx_menu.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx_menu.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx_menu.request_repaint();
                let _ = tx_menu.send(TrayAction::ShowWindow);
            } else if id == &id_hide {
                set_main_window_visible(false);
                let _ = tx_menu.send(TrayAction::HideWindow);
            } else if id == &id_start {
                let _ = tx_menu.send(TrayAction::StartCollector);
                ctx_menu.request_repaint();
            } else if id == &id_stop {
                let _ = tx_menu.send(TrayAction::StopCollector);
                ctx_menu.request_repaint();
            } else if id == &id_quit_ui {
                quit_flag_menu.store(true, Ordering::SeqCst);
                let _ = tx_menu.send(TrayAction::QuitUiOnly);
                ctx_menu.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx_menu.send_viewport_cmd(egui::ViewportCommand::Close);
                ctx_menu.request_repaint();
                // 兜底：若窗口处于隐藏态导致事件循环未响应 Close，短暂等待后直接退出
                thread::spawn(|| {
                    thread::sleep(Duration::from_millis(450));
                    std::process::exit(0);
                });
            } else if id == &id_quit_all {
                quit_flag_menu.store(true, Ordering::SeqCst);
                on_stop_all();
                let _ = tx_menu.send(TrayAction::QuitAll);
                ctx_menu.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx_menu.send_viewport_cmd(egui::ViewportCommand::Close);
                ctx_menu.request_repaint();
                thread::spawn(|| {
                    thread::sleep(Duration::from_millis(600));
                    std::process::exit(0);
                });
            }
        }));

        let tx_icon = tx;
        let ctx_icon = ctx;
        TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
            if matches!(
                ev,
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } | TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                set_main_window_visible(true);
                ctx_icon.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx_icon.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx_icon.request_repaint();
                let _ = tx_icon.send(TrayAction::ShowWindow);
            }
        }));

        Ok(Self {
            _tray: tray,
            rx,
            allow_quit,
        })
    }

    /// 非阻塞读取托盘事件队列
    pub fn poll_actions(&self) -> Vec<TrayAction> {
        let mut actions = Vec::new();
        while let Ok(act) = self.rx.try_recv() {
            actions.push(act);
        }
        actions
    }
}

/// 从内嵌 32px PNG 图标解码构造系统托盘图标，与桌面窗口及 EXE 图标保持完全一致
fn build_rgba_icon() -> Result<Icon, String> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/gpucap-32.png"))
        .map_err(|e| format!("解码托盘图标失败：{e}"))?;
    Icon::from_rgba(icon.rgba, icon.width, icon.height)
        .map_err(|e| format!("构造托盘图标失败：{e}"))
}
