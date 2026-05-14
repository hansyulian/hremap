mod actions;
mod config;
mod io;
mod utils;
mod watcher;

use tokio::sync::watch;

fn detect_de() -> &'static str {
    let xdg = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let xdg = xdg.to_lowercase();
    if xdg.contains("kde") || xdg.contains("plasma") {
        "kde"
    } else if xdg.contains("gnome") || xdg.contains("unity") || xdg.contains("pop") {
        "gnome"
    } else {
        // fallback: check DESKTOP_SESSION too
        let session = std::env::var("DESKTOP_SESSION").unwrap_or_default();
        let session = session.to_lowercase();
        if session.contains("plasma") || session.contains("kde") {
            "kde"
        } else {
            "gnome"
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::debug!("hremap starting...");

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.yaml".to_string());

    tracing::debug!("Loading config from: {}", config_path);

    let cfg = config::load(&config_path)?;
    tracing::debug!(
        "Config loaded: {} layers, {} profiles",
        cfg.layers.len(),
        cfg.profile_map.len()
    );

    let (window_tx, window_rx) = watch::channel(None);

    let de = detect_de();
    tracing::debug!("Detected desktop environment: {}", de);

    tokio::select! {
        result = io::run(window_rx, cfg) => {
            if let Err(e) = result {
                tracing::error!("Input error: {}", e);
            }
        }
        result = async move {
            match de {
                "kde" => watcher::kde::watch(window_tx).await,
                "gnome" => watcher::gnome::watch(window_tx).await,
                _ => Err(anyhow::anyhow!("Unsupported DE: {}", de)),
            }
        } => {
            if let Err(e) = result {
                tracing::error!("Watcher error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Ctrl+C received, ungrabbing devices...");
        }
    }

    tracing::debug!("Devices ungrabbed, exiting cleanly.");
    Ok(())
}
