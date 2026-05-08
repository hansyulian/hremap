use anyhow::Result;
use futures_util::StreamExt;
use tokio::sync::watch;
use zbus::proxy;
use zbus::Connection;

use super::types::WindowInfo;

#[proxy(
    interface = "org.gnome.shell.extensions.FocusedWindow",
    default_service = "org.gnome.Shell",
    default_path = "/org/gnome/shell/extensions/FocusedWindow"
)]
trait FocusedWindow {
    fn get(&self) -> zbus::Result<String>;

    #[zbus(signal)]
    fn focus_changed(&self, window: String) -> zbus::Result<()>;
}

pub async fn watch(tx: watch::Sender<Option<WindowInfo>>) -> Result<()> {
    let conn = Connection::session().await?;
    let proxy = FocusedWindowProxy::new(&conn).await?;

    // get current window immediately on startup
    if let Ok(json) = proxy.get().await {
        if let Ok(info) = serde_json::from_str::<WindowInfo>(&json) {
            tracing::info!("Initial window: {:?}", info);
            let _ = tx.send(Some(info));
        }
    }

    // subscribe to focus changes
    let mut stream = proxy.receive_focus_changed().await?;

    tracing::info!("GNOME watcher listening for focus changes...");

    while let Some(signal) = stream.next().await {
        if let Ok(args) = signal.args() {
            if let Ok(info) = serde_json::from_str::<WindowInfo>(&args.window) {
                tracing::info!("Window changed: {:?}", info);
                let _ = tx.send(Some(info));
            }
        }
    }

    Ok(())
}
