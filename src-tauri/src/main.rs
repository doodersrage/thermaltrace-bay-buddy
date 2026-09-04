// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK on NVIDIA + Wayland aborts with:
    // "Could not create GBM EGL display: EGL_SUCCESS"
    // unless DMA-BUF renderer is disabled. Safe no-op elsewhere.
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            // SAFETY: set before any GTK/WebKit init; single-threaded startup.
            unsafe {
                std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            }
        }
    }

    bay_buddy_lib::run()
}
