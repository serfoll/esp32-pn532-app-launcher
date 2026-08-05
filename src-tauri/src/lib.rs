pub mod catalog;
mod commands;
pub mod launch;
pub mod scan;
pub mod serial;

use serial::{ReaderState, WatchdogEvent};
use std::sync::Mutex;
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

fn show_and_focus_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Env files live at the repo root (sibling to package.json), not
    // src-tauri/, so resolve relative to the compiled-in manifest dir
    // rather than whatever the process's cwd happens to be at launch.
    // .env.local (Vite convention, already covered by the repo's *.local
    // gitignore rule) takes priority over .env when both exist.
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let _ = dotenvy::from_path(repo_root.join(".env.local"))
        .or_else(|_| dotenvy::from_path(repo_root.join(".env")));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(ReaderState::Disconnected))
        .invoke_handler(tauri::generate_handler![
            commands::get_catalog,
            commands::scan_folder,
            commands::add_root_folder,
            commands::remove_root_folder,
            commands::confirm_games,
            commands::refresh_all_artwork,
            commands::rename_game,
            commands::set_custom_artwork,
            commands::bind_tag,
            commands::unbind_tag,
            commands::update_settings,
            commands::launch_game,
            commands::get_reader_state,
            commands::resolve_close_prompt,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                serial::run_watchdog(move |event| match event {
                    WatchdogEvent::State(state) => {
                        *handle.state::<Mutex<ReaderState>>().lock().unwrap() = state;
                        let _ = handle.emit("reader-state", state.as_str());
                    }
                    WatchdogEvent::Tag(serial::ProtocolEvent::Inserted(uid)) => {
                        let _ = handle.emit("tag-inserted", uid);
                    }
                    WatchdogEvent::Tag(serial::ProtocolEvent::Removed(uid)) => {
                        let _ = handle.emit("tag-removed", uid);
                    }
                    WatchdogEvent::Tag(serial::ProtocolEvent::Error(msg)) => {
                        let _ = handle.emit("reader-error", msg);
                    }
                    WatchdogEvent::Tag(_) => {}
                });
            });

            // System tray: the app's whole point is watching the reader in
            // the background, so closing the window shouldn't necessarily
            // quit it -- but hiding the window with no way back except Task
            // Manager would be worse. The tray icon is that way back.
            let tray_menu = MenuBuilder::new(app)
                .text("tray-show", "Show")
                .text("tray-quit", "Quit")
                .build()?;
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("no default window icon configured")?;
            TrayIconBuilder::new()
                .icon(icon)
                .tooltip("Cart Reader")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "tray-show" => show_and_focus_main(app),
                    "tray-quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Down,
                        ..
                    } = event
                    {
                        show_and_focus_main(tray.app_handle());
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            let WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };

            let app = window.app_handle();
            let close_behavior = commands::load_catalog(app)
                .map(|c| c.settings.close_behavior)
                .unwrap_or_default();

            match close_behavior {
                // Decided: minimize to tray -- keep watching the reader.
                catalog::CloseBehavior::Minimize => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                // Decided: quit -- let the close proceed normally.
                catalog::CloseBehavior::Quit => {}
                // Not decided yet: hold the close and let the frontend ask,
                // via resolve_close_prompt once the user answers.
                catalog::CloseBehavior::Ask => {
                    api.prevent_close();
                    let _ = app.emit("close-requested", ());
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
