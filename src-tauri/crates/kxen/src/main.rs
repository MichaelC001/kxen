//! kxen：无头 server bin（不链接 tauri/GUI）。
//! 完整应用服务经单一 HTTP 端点对外（GET /ws + dist 静态托管），浏览器（含 tailscale 远端）凭带 token 的 URL 使用全部功能。

mod args;
mod notify_sink;

use std::process::ExitCode;
use std::sync::Arc;

use kxen_gui::AppState;
use kxen_gui::web::WebServer;

fn main() -> ExitCode {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let cli = match args::parse(std::env::args().skip(1)) {
        Ok(args::Parsed::Help) => {
            print!("{}", args::HELP);
            return ExitCode::SUCCESS;
        }
        Ok(args::Parsed::Run(cli)) => cli,
        Err(error) => {
            eprintln!("error: {error}\n\n{}", args::HELP);
            return ExitCode::from(2);
        }
    };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
    runtime.block_on(run(cli))
}

async fn run(cli: args::Cli) -> ExitCode {
    if !cli.bind.is_loopback() {
        tracing::warn!(bind = %cli.bind, "non-loopback bind exposes the service to the network; the token is the only auth");
    }
    // workdir = 当前工作目录（initial_workdir：cwd 可写即用，/ 或不可写回退 home）
    let mut state = match AppState::new() {
        Ok(state) => state,
        Err(error) => {
            tracing::error!(%error, "app state initialization failed");
            return ExitCode::FAILURE;
        }
    };
    if let Some(token) = &cli.token {
        state.ws_token = token.clone();
    }
    let state = Arc::new(state);
    // 崩溃前排队的消息恢复续跑；teammate -> lead 与 background late 通知的续跑触发（与桌面 setup 等价）
    kxen_gui::ws::pending::restore_queues(state.clone());
    kxen_gui::ws::pending::wire_team_kick(&state);
    kxen_gui::ws::pending::wire_background_kick(&state);
    // 通知落盘 + notification hook（无 OS 通知：AppState 默认 NoopNotify 保持不动）
    notify_sink::spawn(state.clone());
    // cron 与 Knowledge consolidation 使用独立时钟和任务。Provider 慢请求不得阻塞定时消息。
    kxen_gui::background_jobs::spawn(state.clone());
    // MCP servers：信任门 + 双 scope 加载后台启动（server 冷启动可至 60s，绝不阻塞启动路径）
    {
        let state = state.clone();
        tokio::spawn(async move {
            let workdir = kxen_gui::core::shared::read(&state.active_workspace).clone();
            if let Err(error) = state.workspace_runtimes.ready(&workdir).await {
                tracing::warn!(%error, "initial workspace runtime failed");
            }
        });
    }
    // 显式端口语义：占用即报错退出（不静默回退随机端口，书签化 URL 不能漂）
    let handle = match WebServer::start((cli.bind, cli.port), state.clone(), true, cli.allow_hosts.clone()) {
        Ok(handle) => handle,
        Err(error) => {
            tracing::error!(%error, bind = %cli.bind, port = cli.port, "web server bind failed (address already in use?)");
            return ExitCode::FAILURE;
        }
    };
    *kxen_gui::core::shared::lock(&state.ws_port) = handle.port();
    print_banner(&cli, handle.port(), &state.ws_token);
    if tokio::signal::ctrl_c().await.is_ok() {
        tracing::info!("shutdown requested");
    }
    handle.shutdown();
    ExitCode::SUCCESS
}

fn print_banner(cli: &args::Cli, port: u16, token: &str) {
    let bind = cli.bind;
    println!("kxen listening on http://{bind}:{port}/");
    println!();
    println!("  open in browser (keep this URL secret, it carries the only auth token):");
    println!("  http://{bind}:{port}/?token={token}");
    if !bind.is_loopback() || !cli.allow_hosts.is_empty() {
        println!();
        println!("  remote access: terminate TLS with `tailscale serve` instead of exposing plain HTTP");
    }
}
