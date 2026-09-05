mod auth;
mod open_url;

use auth::{
    disconnect_companion, fetch_buddy_state, has_companion_session, start_companion_login,
};
use open_url::open_https_url;

#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    open_https_url(&url)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            open_external,
            start_companion_login,
            disconnect_companion,
            has_companion_session,
            fetch_buddy_state
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bay Buddy");
}
