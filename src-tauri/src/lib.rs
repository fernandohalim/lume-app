mod auth;
mod color;
mod config;
mod http;
mod lyrics;
mod spotify;

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

// Bring the main window back from the tray / minimized state.
fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // Remembers window position/size across launches.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        // System tray: left-click toggles the window; the menu has Show + Quit.
        // (Quit is the only real exit — the close button just hides to the tray.)
        .setup(|app| {
            let show = MenuItemBuilder::with_id("show", "Show lūme").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit lūme").build(app)?;
            let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;

            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("lūme")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // Left-click the tray icon → toggle hide/show.
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                show_main(app);
                            }
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        // Closing the window hides it to the tray instead of quitting; the app
        // keeps running. Real exit is via the tray's Quit (or ⌘Q on macOS).
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
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
            spotify::get_queue,
            spotify::search,
            spotify::add_to_queue,
            spotify::skip_forward,
            lyrics::get_lyrics,
            color::get_accent,
            config::is_configured,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
