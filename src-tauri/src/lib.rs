pub mod catalog;
mod commands;
pub mod launch;
pub mod scan;
pub mod serial;

use serial::{ReaderState, WatchdogEvent};
use std::sync::Mutex;
use tauri::{Emitter, Manager};

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
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
