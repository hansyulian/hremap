use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;
use zbus::Connection;

use super::types::WindowInfo;

const KWIN_SCRIPT: &str = r#"
workspace.windowActivated.connect(function(window) {
    if (!window) return;
    callDBus(
        "org.kde.WindowWatcher",
        "/WindowWatcher",
        "org.kde.WindowWatcher",
        "windowActivated",
        window.caption || "",
        window.resourceClass || "",
        window.resourceName || "",
        window.pid || 0
    );
});
"#;

fn script_path() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").expect("HOME not set");
            PathBuf::from(home).join(".local/share")
        });
    base.join("hremap").join("kwin-watcher.js")
}

fn ensure_script() -> anyhow::Result<PathBuf> {
    let path = script_path();
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if current != KWIN_SCRIPT {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, KWIN_SCRIPT)?;
        tracing::debug!("KWin script written to: {}", path.display());
    } else {
        tracing::debug!("KWin script already up to date at: {}", path.display());
    }
    Ok(path)
}

async fn load_kwin_script() -> anyhow::Result<()> {
    let path = ensure_script()?;
    let path_str = path.to_str().unwrap().to_string();

    let conn = Connection::session().await?;
    let plugin_name = "hremap-window-watcher";

    let is_loaded: bool = conn
        .call_method(
            Some("org.kde.KWin"),
            "/Scripting",
            Some("org.kde.kwin.Scripting"),
            "isScriptLoaded",
            &(plugin_name,),
        )
        .await?
        .body()
        .deserialize()?;

    if is_loaded {
        tracing::debug!("Unloading previous KWin script instance...");
        conn.call_method(
            Some("org.kde.KWin"),
            "/Scripting",
            Some("org.kde.kwin.Scripting"),
            "unloadScript",
            &(plugin_name,),
        )
        .await?;
    }

    let script_id: i32 = conn
        .call_method(
            Some("org.kde.KWin"),
            "/Scripting",
            Some("org.kde.kwin.Scripting"),
            "loadScript",
            &(path_str.as_str(), plugin_name),
        )
        .await?
        .body()
        .deserialize()?;

    if script_id < 0 {
        anyhow::bail!(
            "KWin rejected the script (id={script_id}). Check: {}",
            path_str
        );
    }

    let script_dbus_path = format!("/Scripting/Script{script_id}");
    conn.call_method(
        Some("org.kde.KWin"),
        script_dbus_path.as_str(),
        Some("org.kde.kwin.Script"),
        "run",
        &(),
    )
    .await?;

    tracing::debug!("KWin script loaded and running (id={script_id}).");
    Ok(())
}

fn parse_dbus_value(line: &str) -> Option<String> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix("string \"") {
        return Some(rest.trim_end_matches('"').to_string());
    }
    if let Some(rest) = line.strip_prefix("uint32 ") {
        return Some(rest.trim().to_string());
    }
    if let Some(rest) = line.strip_prefix("int32 ") {
        return Some(rest.trim().to_string());
    }
    None
}

pub async fn watch(tx: watch::Sender<Option<WindowInfo>>) -> anyhow::Result<()> {
    load_kwin_script().await?;

    let mut child = Command::new("dbus-monitor")
        .args(["--session", "interface='org.kde.WindowWatcher'"])
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();

    tracing::debug!("KDE watcher ready.");

    let mut caption = String::new();
    let mut resource_class = String::new();
    let mut resource_name = String::new();
    let mut field = 0usize;

    while let Some(line) = lines.next_line().await? {
        let line = line.trim().to_string();

        if line.contains("member=windowActivated") {
            caption.clear();
            resource_class.clear();
            resource_name.clear();
            field = 0;
            continue;
        }

        if let Some(val) = parse_dbus_value(&line) {
            match field {
                0 => caption = val,
                1 => resource_class = val,
                2 => resource_name = val,
                3 => {
                    let pid: u32 = val.parse().unwrap_or(0);
                    let info = WindowInfo {
                        title: caption.clone(),
                        wm_class: resource_class.clone(),
                        wm_class_instance: resource_name.clone(),
                        pid,
                    };
                    tracing::debug!(
                        "KDE window activated: class=\"{}\" title=\"{}\" pid={}",
                        info.wm_class,
                        info.title,
                        info.pid
                    );
                    tx.send(Some(info)).ok();
                    field = 0;
                    continue;
                }
                _ => {}
            }
            field += 1;
        }
    }

    anyhow::bail!("dbus-monitor exited unexpectedly");
}
