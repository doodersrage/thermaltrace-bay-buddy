use std::process::{Command, Stdio};

/// Open an http(s) URL in the host browser.
///
/// AppImages set `LD_LIBRARY_PATH` / `APPDIR` for their bundled libs. If those
/// leak into `xdg-open` / Chromium, the browser can exit instantly — which looks
/// like the in-app button "does nothing".
#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Only http(s) links can be opened".into());
    }

    #[cfg(target_os = "linux")]
    {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        for key in [
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "APPDIR",
            "APPIMAGE",
            "ARGV0",
            "OWD",
            "PYTHONHOME",
            "PYTHONPATH",
            "PERLLIB",
            "GSETTINGS_SCHEMA_DIR",
            "QT_PLUGIN_PATH",
            "GST_PLUGIN_SYSTEM_PATH",
            "GST_PLUGIN_SYSTEM_PATH_1_0",
        ] {
            cmd.env_remove(key);
        }

        cmd.spawn()
            .map_err(|e| format!("Failed to open browser ({e})"))?;
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        tauri_plugin_opener::open_url(&url, None::<&str>)
            .map_err(|e| format!("Failed to open browser ({e})"))?;
        Ok(())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![open_external])
        .run(tauri::generate_context!())
        .expect("error while running Bay Buddy");
}
