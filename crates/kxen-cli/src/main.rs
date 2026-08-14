//! kxen：无头 server bin（不链接 tauri/GUI）。
//! 完整应用服务经单一 HTTP 端点对外（GET /ws + dist 静态托管），浏览器（含 tailscale 远端）凭带 token 的 URL 使用全部功能。

mod args;

use std::process::ExitCode;
use std::sync::Arc;

use kxen_core::AppState;
use kxen_core::web::WebServer;

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

async fn run(mut cli: args::Cli) -> ExitCode {
    if !cli.bind.is_loopback() {
        tracing::warn!(bind = %cli.bind, "non-loopback bind exposes the service to the network; the token is the only auth");
    }
    // initial_workdir：cwd 可写则用，否则回退 home（含 /）
    let mut state = match AppState::new() {
        Ok(state) => state,
        Err(error) => {
            tracing::error!(%error, "app state initialization failed");
            return ExitCode::FAILURE;
        }
    };
    if let Some(token) = cli.token.take() {
        state.ws_token = token;
    }
    let state = Arc::new(state);
    // 恢复崩溃前排队消息，并接线 teammate/background 续跑（与桌面 setup 等价）
    kxen_core::ws::pending::restore_queues(state.clone());
    kxen_core::ws::pending::wire_team_kick(&state);
    kxen_core::ws::pending::wire_background_kick(&state);
    // 通知落盘与 hook；headless 保持默认 NoopNotify，无 OS 通知
    kxen_core::notify_sink::spawn(state.clone());
    // cron / Knowledge consolidation 独立任务：Provider 慢请求不得阻塞定时消息
    kxen_core::background_jobs::spawn(state.clone());
    // MCP 冷启动可至 60s，信任门与双 scope 加载必须后台，不阻塞启动路径
    {
        let state = state.clone();
        tokio::spawn(async move {
            let workdir = kxen_core::core::shared::read(&state.active_workspace).clone();
            if let Err(error) = state.workspace_runtimes.ready(&workdir).await {
                tracing::warn!(%error, "initial workspace runtime failed");
            }
        });
    }
    // 端口占用即失败退出：不静默回退随机端口，书签化 URL 不能漂
    let remote_access = !cli.bind.is_loopback() || !cli.allow_hosts.is_empty();
    let handle = match WebServer::start((cli.bind, cli.port), state.clone(), true, std::mem::take(&mut cli.allow_hosts)) {
        Ok(handle) => handle,
        Err(error) => {
            tracing::error!(%error, bind = %cli.bind, port = cli.port, "web server bind failed (address already in use?)");
            return ExitCode::FAILURE;
        }
    };
    *kxen_core::core::shared::lock(&state.ws_port) = handle.port();
    print_banner(&cli, handle.port(), &state.ws_token, remote_access);
    if tokio::signal::ctrl_c().await.is_ok() {
        tracing::info!("shutdown requested");
    }
    handle.shutdown();
    ExitCode::SUCCESS
}

fn print_banner(cli: &args::Cli, port: u16, token: &str, remote_access: bool) {
    let bind = cli.bind;
    // IPv6 字面量在 URL host 位置必须带方括号
    let host = if bind.is_ipv6() { format!("[{bind}]") } else { bind.to_string() };
    println!("kxen listening on http://{host}:{port}/");
    println!();
    println!("  open in browser (keep this URL secret, it carries the only auth token):");
    println!("  http://{host}:{port}/?{}", kxen_core::web::token_query(token));
    if remote_access {
        println!();
        println!("  remote access: terminate TLS with `tailscale serve` instead of exposing plain HTTP");
    }
}
