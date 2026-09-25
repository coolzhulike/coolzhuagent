use std::{env, net::SocketAddr};

use axum::{routing::get, Router};
use coolzhu_clawbot_sidecar::{
    health_handler, login_refresh_handler, logout_handler, provider_probe_request_from_env,
    provider_selection_from_env, run_provider_probe, spawn_polling_loop, tick_handler,
    AnyClawbotProvider, ClawbotSidecarConfig, SidecarRuntime,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "coolzhu-clawbot-sidecar\n\n  serve: 默认启动 sidecar HTTP 服务\n  probe: --probe-provider 只探测 provider 并输出 JSON 报告\n\n常用环境变量：\n  COOLZHU_CLAWBOT_PROVIDER_KIND=mock|http\n  COOLZHU_CLAWBOT_PROVIDER_URL=http://127.0.0.1:8790\n  COOLZHU_CLAWBOT_PROVIDER_TOKEN=<secret>\n  COOLZHU_CLAWBOT_ACCOUNT_ID=<account>\n  COOLZHU_CLAWBOT_PROBE_PEER_ID=<peer>\n  COOLZHU_CLAWBOT_PROBE_TEXT=<text>"
        );
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "coolzhu_clawbot_sidecar=info".into()),
        )
        .init();

    let config = ClawbotSidecarConfig::from_env();
    if args.iter().any(|arg| arg == "--probe-provider") {
        let selection = provider_selection_from_env()?;
        let request = provider_probe_request_from_env();
        let report = tokio::task::spawn_blocking(move || {
            let provider = AnyClawbotProvider::from_selection(selection);
            run_provider_probe(&provider, &config, request)
        })
        .await?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        if !report.ok {
            std::process::exit(2);
        }
        return Ok(());
    }

    let bind_addr: SocketAddr = config.bind_addr.parse()?;
    let provider_selection = provider_selection_from_env()?;
    let provider =
        tokio::task::spawn_blocking(move || AnyClawbotProvider::from_selection(provider_selection))
            .await?;
    let runtime = SidecarRuntime::new(config, provider);
    let _polling_task = spawn_polling_loop(runtime.clone());
    let app = Router::new()
        .route("/health", get(health_handler).post(tick_handler))
        .route("/version", get(health_handler))
        .route("/tick", get(tick_handler).post(tick_handler))
        .route("/login/refresh", axum::routing::post(login_refresh_handler))
        .route("/login/logout", axum::routing::post(logout_handler))
        .with_state(runtime);

    tracing::info!("ClawBot sidecar listening on {bind_addr}");
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
