use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};

/// Build an environment safe for launching host tools from an AppImage.
///
/// AppImages (especially linuxdeploy GTK hooks) prefix `PATH` with a bundled
/// `xdg-open`, set `XDG_DATA_DIRS` / GTK / GIO vars into `$APPDIR`, and inject
/// `LD_LIBRARY_PATH`. Spawning the host browser with that env often "succeeds"
/// while the browser never appears.
fn host_process_env() -> HashMap<String, String> {
    let appdir = std::env::var_os("APPDIR").map(|v| v.to_string_lossy().into_owned());
    let mut env: HashMap<String, String> = std::env::vars().collect();

    for key in [
        "APPDIR",
        "APPIMAGE",
        "ARGV0",
        "OWD",
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
        "PYTHONHOME",
        "PYTHONPATH",
        "PERLLIB",
        "GSETTINGS_SCHEMA_DIR",
        "QT_PLUGIN_PATH",
        "GST_PLUGIN_SYSTEM_PATH",
        "GST_PLUGIN_SYSTEM_PATH_1_0",
        "GTK_DATA_PREFIX",
        "GTK_EXE_PREFIX",
        "GTK_PATH",
        "GTK_IM_MODULE_FILE",
        "GDK_PIXBUF_MODULE_FILE",
        "GIO_EXTRA_MODULES",
        "GI_TYPELIB_PATH",
        "GDK_BACKEND",
        "GTK_THEME",
    ] {
        env.remove(key);
    }

    for key in ["PATH", "XDG_DATA_DIRS", "XDG_CONFIG_DIRS", "QT_PLUGIN_PATH"] {
        if let Some(value) = env.get(key).cloned() {
            let cleaned = value
                .split(':')
                .filter(|part| {
                    if part.is_empty() {
                        return false;
                    }
                    if part.contains("/tmp/.mount") || part.contains("appimage_extracted") {
                        return false;
                    }
                    if let Some(appdir) = appdir.as_deref() {
                        if part.starts_with(appdir) {
                            return false;
                        }
                    }
                    true
                })
                .collect::<Vec<_>>()
                .join(":");
            if cleaned.is_empty() {
                env.remove(key);
            } else {
                env.insert(key.to_string(), cleaned);
            }
        }
    }

    // Prefer a predictable host PATH so we never pick AppDir's bundled xdg-open.
    let path = env
        .get("PATH")
        .map(|p| format!("/usr/local/bin:/usr/bin:/bin:{p}"))
        .unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into());
    env.insert("PATH".into(), path);

    env
}

fn spawn_host(program: &str, args: &[&str], env: &HashMap<String, String>) -> Result<(), String> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env_clear()
        .envs(env)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{program}: {e}"))
}

/// Open an http(s) URL in the host browser.
#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Only http(s) links can be opened".into());
    }

    #[cfg(target_os = "linux")]
    {
        let env = host_process_env();
        let mut errors = Vec::new();

        // Prefer absolute host binaries — never AppDir/usr/bin/xdg-open.
        for (bin, args) in [
            ("/usr/bin/xdg-open", vec![url.as_str()]),
            ("/usr/bin/gio", vec!["open", url.as_str()]),
            ("xdg-open", vec![url.as_str()]),
        ] {
            if bin.starts_with('/') && !Path::new(bin).is_file() {
                continue;
            }
            match spawn_host(bin, &args, &env) {
                Ok(()) => return Ok(()),
                Err(e) => errors.push(e),
            }
        }

        // xdg-desktop-portal OpenURI as last resort
        let portal = Command::new("/usr/bin/gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                "org.freedesktop.portal.Desktop",
                "--object-path",
                "/org/freedesktop/portal/desktop",
                "--method",
                "org.freedesktop.portal.OpenURI.OpenURI",
                "",
                &url,
                "{}",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env_clear()
            .envs(&env)
            .status();

        match portal {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => errors.push(format!("portal OpenURI exited {status}")),
            Err(e) => errors.push(format!("portal: {e}")),
        }

        Err(format!(
            "Could not open browser ({})",
            errors.join("; ")
        ))
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
