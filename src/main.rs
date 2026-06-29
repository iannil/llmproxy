use anyhow::Context;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;

/// llmproxy — Unified model API bridge
///
/// A lightweight proxy that translates between different AI model API protocols
/// (Anthropic Messages, OpenAI Chat/Responses, Google Gemini), allowing any
/// CLI tool to use any model provider.
#[derive(Parser, Debug)]
#[command(name = "llmproxy", version, about)]
struct Args {
    /// Path to YAML configuration file
    #[arg(short, long, default_value = "config.yaml")]
    config: PathBuf,

    /// Listen address (overrides config file)
    #[arg(short, long)]
    listen: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    log::info!("Loading config from: {:?}", args.config);
    let config_state = llmproxy::config::ConfigState::load(&args.config)
        .await
        .context("Failed to load configuration")?;

    let listen = if let Some(ref addr) = args.listen {
        addr.clone()
    } else {
        config_state.listen_addr().await
    };

    log::info!(
        "llmproxy starting with {} upstream(s) on {}",
        config_state.all_providers().await.len(),
        listen
    );

    for provider in config_state.all_providers().await {
        log::info!("  → {} ({})", provider.display_name(), provider.id);
    }

    // 启动代理服务器
    let config_state = Arc::new(config_state);
    llmproxy::proxy::server::start(config_state.clone()).await;

    // 等待退出信号
    wait_for_shutdown().await;

    log::info!("Shutting down");
    Ok(())
}

async fn wait_for_shutdown() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let term = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let term = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
}
