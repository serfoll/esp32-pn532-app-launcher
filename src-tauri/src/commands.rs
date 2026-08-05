// Tauri commands: the IPC surface the frontend calls into. Thin glue over
// catalog/scan — no business logic lives here beyond wiring app-data paths.

use crate::catalog::{self, Binding, Catalog, CloseBehavior, ConfirmedGame};
use crate::scan;
use crate::serial::ReaderState;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

// pub(crate): lib.rs's window close-event handler needs to read/write the
// close_behavior setting without going through a full Tauri command.
pub(crate) fn catalog_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("catalog.json"))
}

pub(crate) fn load_catalog(app: &AppHandle) -> Result<Catalog, String> {
    catalog::load(&catalog_path(app)?).map_err(|e| e.to_string())
}

pub(crate) fn save_catalog(app: &AppHandle, catalog: &Catalog) -> Result<(), String> {
    catalog::save(&catalog_path(app)?, catalog).map_err(|e| e.to_string())
}

/// Where artwork gets written, and the SteamGridDB key to try first (only
/// if it's actually reachable right now -- see `steamgriddb_reachable`'s
/// doc comment). Shared by every command that resolves or re-resolves
/// artwork so this lookup only lives in one place.
fn artwork_resolution_context(app: &AppHandle) -> Result<(PathBuf, Option<String>), String> {
    let artwork_dir = catalog_path(app)?.parent().unwrap().join("artwork");
    let steamgriddb_key = std::env::var("STEAMGRIDDB_API_KEY")
        .ok()
        .filter(|key| scan::steamgriddb_reachable(key));
    Ok((artwork_dir, steamgriddb_key))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCandidateDto {
    pub folder_path: String,
    pub name: String,
    pub exe_path: Option<String>,
}

/// Loads the catalog, rescanning game availability first so the gallery
/// never shows stale available/unavailable state from a prior session.
#[tauri::command]
pub fn get_catalog(app: AppHandle) -> Result<Catalog, String> {
    let mut catalog = load_catalog(&app)?;
    catalog::rescan_availability(&mut catalog);
    catalog::backfill_stores(&mut catalog);
    save_catalog(&app, &catalog)?;
    Ok(catalog)
}

#[tauri::command]
pub fn scan_folder(path: String) -> Result<Vec<ScanCandidateDto>, String> {
    let root = std::path::Path::new(&path);
    if !root.is_dir() {
        return Err(format!("'{path}' doesn't exist or isn't a folder"));
    }
    if !scan::is_scannable_root(root) {
        return Err(format!("'{path}' is a protected system folder and can't be added"));
    }

    Ok(scan::scan_root(root)
        .into_iter()
        .map(|c| ScanCandidateDto {
            folder_path: c.folder_path,
            name: c.name,
            exe_path: c.exe_path,
        })
        .collect())
}

#[tauri::command]
pub fn add_root_folder(app: AppHandle, path: String) -> Result<Catalog, String> {
    // Defense in depth alongside scan_folder's own check: a root folder
    // that scan_root refuses to scan (drive roots, OS-critical
    // directories) should never even get persisted, since sync would
    // forever ignore it once it's there -- a dead entry sitting in
    // Settings with no way to explain to the user why nothing shows up.
    if !scan::is_scannable_root(std::path::Path::new(&path)) {
        return Err(format!("'{path}' is a protected system folder and can't be added"));
    }

    let mut catalog = load_catalog(&app)?;
    if !catalog.settings.root_folders.contains(&path) {
        catalog.settings.root_folders.push(path);
    }
    // Re-adding a folder that was previously removed: its games never got
    // deleted (removal only marks them unavailable, per this codebase's
    // never-silently-delete rule), so confirm_games' dedup-by-folder-path
    // check just skipped them as "already cataloged" -- nothing else would
    // notice they're valid again until the next full catalog load.
    catalog::rescan_availability(&mut catalog);
    save_catalog(&app, &catalog)?;
    Ok(catalog)
}

/// Drops a folder from the root-folders list and rescans availability.
/// Games that came from that folder are left in the catalog untouched
/// (per the spec's Boundaries: never silently delete/mutate catalog
/// entries) — removing the root just stops it from being scanned again;
/// its games naturally show unavailable once their exe stops existing.
#[tauri::command]
pub fn remove_root_folder(app: AppHandle, path: String) -> Result<Catalog, String> {
    let mut catalog = load_catalog(&app)?;
    catalog.settings.root_folders.retain(|p| p != &path);
    catalog::rescan_availability(&mut catalog);
    save_catalog(&app, &catalog)?;
    Ok(catalog)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmResult {
    pub catalog: Catalog,
    pub added: usize,
}

/// Adds every game the frontend already scanned and auto-filtered from one
/// folder (unambiguous exe detected, per `ScanCandidateDto.exePath`).
/// `added` can be less than `games.len()` -- re-adding a previously
/// removed folder finds the same candidates again, but its games were
/// never deleted (only marked unavailable), so add_confirmed_games' dedup
/// skips them here; the frontend needs the real count to report honestly
/// rather than assuming every candidate it sent got added.
#[tauri::command]
pub fn confirm_games(app: AppHandle, games: Vec<ConfirmedGame>) -> Result<ConfirmResult, String> {
    let mut catalog = load_catalog(&app)?;
    let (artwork_dir, steamgriddb_key) = artwork_resolution_context(&app)?;

    let added = catalog::add_confirmed_games(&mut catalog, &artwork_dir, games, steamgriddb_key.as_deref());

    save_catalog(&app, &catalog)?;
    Ok(ConfirmResult { catalog, added })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub catalog: Catalog,
    pub added: usize,
    pub skipped_names: Vec<String>,
}

/// Rescans every already-registered root folder and adds any game not yet
/// in the catalog -- the gallery's "Sync" action. Only picks up candidates
/// with an unambiguous exe (same auto-detection `scan_folder` already
/// does); a folder `scan_new_games` can't confidently name an exe for goes
/// into `skipped_names` instead, since there's no review step left to ask
/// the user which one to use.
#[tauri::command]
pub fn sync_library(app: AppHandle) -> Result<SyncResult, String> {
    let mut catalog = load_catalog(&app)?;
    let (artwork_dir, steamgriddb_key) = artwork_resolution_context(&app)?;

    let known_folder_paths: Vec<String> =
        catalog.games.iter().map(|g| g.folder_path.clone()).collect();
    let candidates = scan::scan_new_games(&catalog.settings.root_folders, &known_folder_paths);
    // scan_new_games only omits a candidate for being already-known; an
    // undetected exe still comes back with exe_path: None here, so this is
    // the one place left that can tell "found but unaddable" apart from
    // "nothing new at all" -- and report which ones, rather than trusting
    // a scan_new_games invariant this file would otherwise have to unwrap
    // blindly.
    let mut new_games = Vec::new();
    let mut skipped_names = Vec::new();
    for c in candidates {
        match c.exe_path {
            Some(exe_path) => new_games.push(ConfirmedGame {
                folder_path: c.folder_path,
                name: c.name,
                exe_path,
            }),
            None => skipped_names.push(c.name),
        }
    }

    let added = catalog::add_confirmed_games(&mut catalog, &artwork_dir, new_games, steamgriddb_key.as_deref());

    save_catalog(&app, &catalog)?;
    Ok(SyncResult { catalog, added, skipped_names })
}

/// Re-resolves artwork for every already-cataloged game, overwriting the
/// existing file at the same artwork path. For games added before
/// SteamGridDB was wired in (or before a key was configured), this is the
/// only way to upgrade their art without removing and re-adding them.
/// Skips games with a user-uploaded custom art override — refreshing must
/// never silently replace something the user picked deliberately.
#[tauri::command]
pub fn refresh_all_artwork(app: AppHandle) -> Result<Catalog, String> {
    let mut catalog = load_catalog(&app)?;
    let (artwork_dir, steamgriddb_key) = artwork_resolution_context(&app)?;

    catalog::backfill_stores(&mut catalog);

    for game in &mut catalog.games {
        if game.has_custom_artwork {
            continue;
        }
        let dest = artwork_dir.join(format!("{}.png", catalog::sanitize_for_filename(&game.id)));
        if let Some(path) = catalog::resolve_game_artwork(
            &game.name,
            &game.folder_path,
            &game.exe_path,
            &dest,
            steamgriddb_key.as_deref(),
        ) {
            game.artwork_path = Some(path.to_string_lossy().to_string());
        }
    }

    save_catalog(&app, &catalog)?;
    Ok(catalog)
}

/// Renames a game. Identity (`id`, used by bindings) is the folder path,
/// not the name, so renaming never touches existing tag bindings.
#[tauri::command]
pub fn rename_game(app: AppHandle, game_id: String, name: String) -> Result<Catalog, String> {
    let mut catalog = load_catalog(&app)?;
    let game = catalog
        .games
        .iter_mut()
        .find(|g| g.id == game_id)
        .ok_or_else(|| format!("no game with id '{game_id}'"))?;
    game.name = name;
    save_catalog(&app, &catalog)?;
    Ok(catalog)
}

/// Copies a user-picked image to the game's artwork slot and marks it as a
/// custom override so `refresh_all_artwork` never overwrites it again.
#[tauri::command]
pub fn set_custom_artwork(
    app: AppHandle,
    game_id: String,
    source_path: String,
) -> Result<Catalog, String> {
    let mut catalog = load_catalog(&app)?;
    let artwork_dir = catalog_path(&app)?.parent().unwrap().join("artwork");
    std::fs::create_dir_all(&artwork_dir).map_err(|e| e.to_string())?;

    let game = catalog
        .games
        .iter_mut()
        .find(|g| g.id == game_id)
        .ok_or_else(|| format!("no game with id '{game_id}'"))?;

    let dest = artwork_dir.join(format!("{}.png", catalog::sanitize_for_filename(&game.id)));
    let image = image::open(&source_path).map_err(|e| e.to_string())?;
    image.save(&dest).map_err(|e| e.to_string())?;

    game.artwork_path = Some(dest.to_string_lossy().to_string());
    game.has_custom_artwork = true;
    save_catalog(&app, &catalog)?;
    Ok(catalog)
}

/// Binds a tag to a game, replacing any prior binding for that tag.
#[tauri::command]
pub fn bind_tag(app: AppHandle, tag_uid: String, game_id: String) -> Result<Catalog, String> {
    let mut catalog = load_catalog(&app)?;
    catalog.bindings.retain(|b| b.tag_uid != tag_uid);
    catalog.bindings.push(Binding { tag_uid, game_id });
    save_catalog(&app, &catalog)?;
    Ok(catalog)
}

/// Removes a tag's binding, if any. The game itself is untouched — only
/// the tag<->game link goes away, so the tag reads as unbound (bind-prompt)
/// on its next insert, per the spec's no-silent-catalog-mutation boundary.
#[tauri::command]
pub fn unbind_tag(app: AppHandle, tag_uid: String) -> Result<Catalog, String> {
    let mut catalog = load_catalog(&app)?;
    catalog.bindings.retain(|b| b.tag_uid != tag_uid);
    save_catalog(&app, &catalog)?;
    Ok(catalog)
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    root_folders: Vec<String>,
    confirm_before_launch: bool,
    show_output_log: bool,
    close_behavior: CloseBehavior,
    show_store_badges: bool,
    sync_on_startup: bool,
) -> Result<Catalog, String> {
    let mut catalog = load_catalog(&app)?;
    catalog.settings.root_folders = root_folders;
    catalog.settings.confirm_before_launch = confirm_before_launch;
    catalog.settings.show_output_log = show_output_log;
    catalog.settings.close_behavior = close_behavior;
    catalog.settings.show_store_badges = show_store_badges;
    catalog.settings.sync_on_startup = sync_on_startup;
    save_catalog(&app, &catalog)?;
    Ok(catalog)
}

/// Returns the watchdog thread's last-observed state. The frontend needs
/// this on load in addition to listening for future "reader-state" events
/// — a page load that happens after the reader already settled into its
/// current state would otherwise never learn what that state is, since the
/// watchdog only emits on a *change*, not on every poll.
#[tauri::command]
pub fn get_reader_state(state: State<'_, Mutex<ReaderState>>) -> String {
    state.lock().unwrap().as_str().to_string()
}

/// Progress events emitted to the frontend as `flash-progress` while a
/// flash is underway -- internally tagged so the JSON payload is
/// `{"stage":"writing","current":N,"total":N}` / `{"stage":"verifying"}` /
/// `{"stage":"done"}`, one flat shape the frontend can switch on directly.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "stage", rename_all = "camelCase")]
enum FlashProgress {
    Writing { current: usize, total: usize },
    Verifying,
    Done,
}

/// Forwards espflash's progress callbacks to the frontend as Tauri events
/// instead of the crate's own (terminal-only) default reporting, so the
/// app's progress dialog can show real percentage rather than a static
/// indeterminate bar, and the output log can show real stage transitions
/// instead of nothing at all for the whole multi-minute operation.
struct TauriProgress {
    app: AppHandle,
    total: usize,
}

impl TauriProgress {
    fn new(app: AppHandle) -> Self {
        Self { app, total: 0 }
    }

    fn emit(&self, progress: FlashProgress) {
        let _ = self.app.emit("flash-progress", progress);
    }
}

impl espflash::target::ProgressCallbacks for TauriProgress {
    fn init(&mut self, _addr: u32, total: usize) {
        self.total = total;
        self.emit(FlashProgress::Writing { current: 0, total });
    }

    fn update(&mut self, current: usize) {
        self.emit(FlashProgress::Writing { current, total: self.total });
    }

    fn verifying(&mut self) {
        self.emit(FlashProgress::Verifying);
    }

    fn finish(&mut self, _skipped: bool) {
        self.emit(FlashProgress::Done);
    }
}

/// Flashes the app's bundled firmware to the currently-connected reader.
/// Sets the shared `flashing` flag around the (blocking, can take a
/// couple of minutes) flash operation so the watchdog thread steps aside
/// instead of fighting it for the port -- see `serial::run_watchdog`'s
/// doc comment.
///
/// `async` + `spawn_blocking`, not a plain blocking `fn` -- Tauri runs
/// non-async commands on the *main* thread by default, which froze the
/// whole window for the entire flash (confirmed live: title bar showed
/// "Not Responding"). spawn_blocking moves the actual blocking espflash
/// work onto its own dedicated thread instead.
#[tauri::command]
pub async fn flash_firmware(
    app: AppHandle,
    flashing: State<'_, std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(), String> {
    // Acquired before the port lookup, and held across the whole spawned
    // task via the .await below -- two concurrent invocations (e.g. the
    // header button and the dev-only test button, both clickable in a
    // dev build) would otherwise both proceed and contend for the same
    // port mid-write, which could corrupt an in-progress, non-idempotent
    // firmware write.
    let _guard = FlashingGuard::try_new(flashing.inner().clone())?;

    let port = crate::serial::find_reader_port()
        .ok_or_else(|| "No reader connected -- plug in the board first".to_string())?;

    // Falls back to the built-in default until the user pairs a specific
    // device (see pair_reader_device) -- covers the common case (this
    // exact board) without needing pairing to happen first.
    let catalog = load_catalog(&app)?;
    let expected_vid = catalog.settings.reader_usb_vid.unwrap_or(crate::flash::DEFAULT_READER_USB_VID);
    let expected_pid = catalog.settings.reader_usb_pid.unwrap_or(crate::flash::DEFAULT_READER_USB_PID);

    tauri::async_runtime::spawn_blocking(move || {
        let mut progress = TauriProgress::new(app);
        crate::flash::flash_firmware(&port, expected_vid, expected_pid, &mut progress)
    })
    .await
    .map_err(|e| format!("flash task panicked: {e}"))?
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbSerialPortInfo {
    pub port_name: String,
    pub vid: u16,
    pub pid: u16,
    pub description: String,
}

/// Lists every currently-connected USB serial device, for the reader
/// pairing dialog -- a mismatch between the connected device and
/// `Settings.readerUsbVid`/`Pid` (or the built-in default) is what sends
/// the frontend here in the first place, so the user can confirm which
/// one is actually their reader.
#[tauri::command]
pub fn list_usb_serial_ports() -> Result<Vec<UsbSerialPortInfo>, String> {
    Ok(serialport::available_ports()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|p| match p.port_type {
            serialport::SerialPortType::UsbPort(info) => Some(UsbSerialPortInfo {
                port_name: p.port_name,
                vid: info.vid,
                pid: info.pid,
                description: info
                    .product
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| info.manufacturer.clone())
                    .unwrap_or_else(|| format!("USB {:04X}:{:04X}", info.vid, info.pid)),
            }),
            _ => None,
        })
        .collect())
}

/// Persists the user's confirmed reader device from the pairing dialog.
/// Trust-on-first-use: once paired, `flash_firmware` only accepts this
/// exact VID/PID instead of the built-in default.
#[tauri::command]
pub fn pair_reader_device(app: AppHandle, vid: u16, pid: u16) -> Result<Catalog, String> {
    let mut catalog = load_catalog(&app)?;
    catalog.settings.reader_usb_vid = Some(vid);
    catalog.settings.reader_usb_pid = Some(pid);
    save_catalog(&app, &catalog)?;
    Ok(catalog)
}

/// Exclusively claims `flashing` for its lifetime (fails rather than
/// overwriting an already-true flag) and always sets it back to false on
/// drop -- unlike a plain store/call/store, this still resets it if
/// `flash::flash_firmware` panics partway through, which a bare pair of
/// statements wouldn't: the second store would simply never run, leaving
/// the watchdog thread paused forever with no way to recover short of
/// restarting the app. Owns the Arc (rather than borrowing) so it can move
/// into the spawn_blocking closure above, which requires 'static.
struct FlashingGuard(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl FlashingGuard {
    fn try_new(flag: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Result<Self, String> {
        flag.compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::Acquire,
            std::sync::atomic::Ordering::Relaxed,
        )
        .map_err(|_| "Firmware flash already in progress".to_string())?;
        Ok(Self(flag))
    }
}

impl Drop for FlashingGuard {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Launches a game, preferring its launcher's own protocol handler over a
/// direct exe spawn when one is detected (currently just Steam) — many
/// launcher-installed games are DRM-wrapped stubs that exit silently
/// outside their launcher instead of showing an error, and going through
/// the launcher sidesteps that entirely.
///
/// Returns `Ok(false)` instead of launching if something from the game's
/// install folder is already running — a cart pulled and reinserted while
/// its game is still open would otherwise spawn a second instance once the
/// tag-event cooldown lapses. Checked by folder rather than the recorded
/// exe specifically, since a launcher stub (EA App, confirmed live) can
/// hand off to a *different* exe in the same folder and exit itself —
/// checking the exact exe would see it gone and wrongly launch again.
///
/// For the direct-spawn fallback: this only reports whether the OS
/// accepted the launch, not whether the game window ever showed up. An
/// earlier version tried to catch an immediate exit as a failure signal,
/// but plenty of legitimate launchers (EA App among them) use a thin stub
/// exe that hands off to the real game and exits *by design* — that's
/// indistinguishable from a genuine crash from the outside, so a "did it
/// stay alive" check produces false failures on working launches. Better
/// to under-report than to tell the user a launch failed when it didn't.
#[tauri::command]
pub fn launch_game(exe_path: String, folder_path: String) -> Result<bool, String> {
    if crate::launch::is_game_running(&folder_path) {
        return Ok(false);
    }

    let path = std::path::Path::new(&exe_path);

    if let Some(folder) = path.parent() {
        if let Some(app_id) = crate::launch::find_steam_app_id(folder) {
            open::that(format!("steam://rungameid/{app_id}")).map_err(|e| e.to_string())?;
            return Ok(true);
        }
    }

    let mut cmd = std::process::Command::new(path);
    if let Some(dir) = path.parent() {
        cmd.current_dir(dir);
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(true)
}

/// Kills a running game's process(es), the inverse of `launch_game` --
/// the gallery's "Stop" action. `killed == 0` isn't an error: it just
/// means the game had already stopped on its own (e.g. the next poll
/// tick raced this call).
#[tauri::command]
pub fn stop_game(folder_path: String) -> bool {
    crate::launch::stop_game(&folder_path) > 0
}

/// Returns the IDs of games with a process currently running from their
/// install folder. Polled by the frontend to keep each game card's
/// "running" badge in sync -- batched through `running_folders` so a
/// library of many games costs one process-list scan per poll, not one per
/// game.
#[tauri::command]
pub fn get_running_games(app: AppHandle) -> Result<Vec<String>, String> {
    let catalog = load_catalog(&app)?;
    let folder_paths: Vec<&str> = catalog.games.iter().map(|g| g.folder_path.as_str()).collect();
    let running = crate::launch::running_folders(folder_paths);
    Ok(catalog
        .games
        .iter()
        .filter(|g| running.contains(&g.folder_path))
        .map(|g| g.id.clone())
        .collect())
}

/// Answers the first-close prompt (lib.rs's window close-event handler
/// shows it by emitting a "close-requested" event instead of letting the
/// window close, whenever `close_behavior` hasn't been decided yet).
/// Persists the choice for next time when `remember` is set, then either
/// hides the window (tray-resident) or actually exits the app.
///
/// Returns the (possibly updated) catalog so the frontend's in-memory
/// settings stay in sync -- otherwise a remembered choice made from this
/// dialog wouldn't show up in the Settings panel until the app restarted.
#[tauri::command]
pub fn resolve_close_prompt(
    app: AppHandle,
    minimize: bool,
    remember: bool,
) -> Result<Catalog, String> {
    let mut catalog = load_catalog(&app)?;
    if remember {
        catalog.settings.close_behavior = if minimize {
            CloseBehavior::Minimize
        } else {
            CloseBehavior::Quit
        };
        save_catalog(&app, &catalog)?;
    }

    if minimize {
        if let Some(window) = app.get_webview_window("main") {
            window.hide().map_err(|e| e.to_string())?;
        }
    } else {
        app.exit(0);
    }
    Ok(catalog)
}
