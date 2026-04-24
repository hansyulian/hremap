mod actions;
mod config;
mod input;
mod watcher;

use tokio::sync::watch;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("hremap starting...");

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.yaml".to_string());

    tracing::info!("Loading config from: {}", config_path);

    let cfg = config::load(&config_path)?;
    tracing::info!(
        "Config loaded: {} layers, {} profiles",
        cfg.layers.len(),
        cfg.profile_map.len()
    );

    let (window_tx, window_rx) = watch::channel(None);
    let window_rx_printer = window_rx.clone();

    tokio::spawn(async move {
        if let Err(e) = watcher::gnome::watch(window_tx).await {
            tracing::error!("Watcher error: {}", e);
        }
    });

    tokio::spawn(async move {
        let mut rx = window_rx_printer;
        loop {
            rx.changed().await.ok();
            if let Some(win) = rx.borrow().clone() {
                println!(
                    "[Active Window] class=\"{}\" title=\"{}\" pid={}",
                    win.wm_class, win.title, win.pid
                );
            }
        }
    });

    tokio::select! {
        result = input::run(window_rx, cfg) => {
            if let Err(e) = result {
                tracing::error!("Input error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Ctrl+C received, ungrabbing devices...");
        }
    }

    tracing::info!("Devices ungrabbed, exiting cleanly.");
    Ok(())
}
