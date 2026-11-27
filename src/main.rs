#![windows_subsystem = "windows"]
//! GPUCap 程序统一入口：采用 GUI 子系统消除启动黑框闪烁，命令行模式自动附加父控制台。

mod collectors;
mod cpu_probe;
mod daemon;
mod infra;
mod live;
mod tray;
mod ui;
mod web;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gpucap", about = "GPU/系统异常采集器与实时日志监控")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 前台运行采集
    Run {
        #[arg(long)]
        minutes: Option<f64>,
    },
    /// 后台运行采集（无窗口）
    Daemon {
        #[arg(long)]
        minutes: Option<f64>,
    },
    /// 停止采集
    Stop,
    /// 查看状态
    Status,
    /// 守护运行（崩溃自动重启）
    Supervise,
    /// 打开实时日志界面
    Ui {
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        selftest: bool,
        #[arg(long)]
        minimized: bool,
    },
    /// 安装开机自启
    InstallStartup,
    /// 移除开机自启
    UninstallStartup,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let is_gui_or_bg = args.len() <= 1
        || matches!(
            args.get(1).map(|s| s.as_str()),
            Some("ui") | Some("daemon") | Some("supervise")
        );
    if !is_gui_or_bg || args.iter().any(|a| a == "--selftest" || a == "-h" || a == "--help") {
        infra::attach_parent_console_if_needed();
    }

    let cli = Cli::parse();
    let code = match cli.cmd {
        Some(Commands::Run { minutes }) => daemon::cmd_run(minutes),
        Some(Commands::Daemon { minutes }) => daemon::cmd_daemon(minutes),
        Some(Commands::Stop) => daemon::cmd_stop(),
        Some(Commands::Status) => daemon::cmd_status(),
        Some(Commands::Supervise) => daemon::cmd_supervise(),
        Some(Commands::Ui {
            session,
            selftest,
            minimized,
        }) => ui::cmd_ui(session, selftest, minimized),
        Some(Commands::InstallStartup) => daemon::cmd_install_startup(),
        Some(Commands::UninstallStartup) => daemon::cmd_uninstall_startup(),
        None => ui::cmd_ui(None, false, false),
    };
    std::process::exit(code);
}
