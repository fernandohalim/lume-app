mod auth;
mod color;
mod spotify;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // Remembers window position/size across launches.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            auth::login,
            auth::logout,
            auth::is_authenticated,
            auth::whoami,
            spotify::get_playback_state,
            spotify::play,
            spotify::pause,
            spotify::next,
            spotify::previous,
            spotify::seek,
            spotify::set_volume,
            spotify::set_shuffle,
            spotify::set_repeat,
            color::get_accent,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
