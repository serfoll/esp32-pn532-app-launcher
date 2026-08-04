// Tauri commands: the IPC surface the frontend calls into. Thin glue over
// catalog/scan — no business logic lives here beyond wiring app-data paths.

use crate::catalog::{self, Binding, Catalog, Game};
use crate::scan;
use crate::serial::ReaderState;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

fn catalog_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("catalog.json"))
}

fn load_catalog(app: &AppHandle) -> Result<Catalog, String> {
    catalog::load(&catalog_path(app)?).map_err(|e| e.to_string())
}

fn save_catalog(app: &AppHandle, catalog: &Catalog) -> Result<(), String> {
    catalog::save(&catalog_path(app)?, catalog).map_err(|e| e.to_string())
}

/// Games use their folder path as `id` — it's already unique per game, so a
/// separate uuid dependency would just be more code for the same guarantee.
fn sanitize_for_filename(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCandidateDto {
    pub folder_path: String,
    pub name: String,
    pub exe_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedGame {
    pub folder_path: String,
    pub name: String,
    pub exe_path: String,
}

/// Loads the catalog, rescanning game availability first so the gallery
/// never shows stale available/unavailable state from a prior session.
#[tauri::command]
pub fn get_catalog(app: AppHandle) -> Result<Catalog, String> {
    let mut catalog = load_catalog(&app)?;
    catalog::rescan_availability(&mut catalog);
    save_catalog(&app, &catalog)?;
    Ok(catalog)
}

#[tauri::command]
pub fn scan_folder(path: String) -> Result<Vec<ScanCandidateDto>, String> {
    let root = std::path::Path::new(&path);
    if !root.is_dir() {
        return Err(format!("'{path}' doesn't exist or isn't a folder"));
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
    let mut catalog = load_catalog(&app)?;
    if !catalog.settings.root_folders.contains(&path) {
        catalog.settings.root_folders.push(path);
    }
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

/// Resolves and writes artwork for one game: SteamGridDB first when a
/// (reachability-checked) key is available, local folder art / exe icon
/// otherwise or on any SteamGridDB failure. Shared by `confirm_games` and
/// `refresh_all_artwork` so the fallback chain only lives in one place.
fn resolve_game_artwork(
    name: &str,
    folder_path: &str,
    exe_path: &str,
    dest: &std::path::Path,
    steamgriddb_key: Option<&str>,
) -> Option<PathBuf> {
    steamgriddb_key
        .and_then(|key| scan::fetch_steamgriddb_icon(name, key, dest))
        .or_else(|| {
            scan::resolve_artwork(
                std::path::Path::new(folder_path),
                Some(std::path::Path::new(exe_path)),
                dest,
            )
        })
}

/// Persists scanned candidates the user has explicitly reviewed and
/// confirmed (per-row exe path, possibly hand-corrected) — this is the
/// only path that adds a Game to the catalog, per the spec's Success
/// Criteria ("no path from scan to bound-and-launchable without a confirm
/// step"). Candidates already in the catalog (same folder path) are
/// skipped rather than duplicated.
#[tauri::command]
pub fn confirm_games(app: AppHandle, games: Vec<ConfirmedGame>) -> Result<Catalog, String> {
    let mut catalog = load_catalog(&app)?;
    let artwork_dir = catalog_path(&app)?.parent().unwrap().join("artwork");
    // Checked once per batch, not per game -- see steamgriddb_reachable's
    // doc comment for why (bounds the offline-batch cost to one short
    // check instead of every game paying its own timeout chain).
    let steamgriddb_key = std::env::var("STEAMGRIDDB_API_KEY")
        .ok()
        .filter(|key| scan::steamgriddb_reachable(key));

    for g in games {
        if catalog
            .games
            .iter()
            .any(|existing| existing.folder_path == g.folder_path)
        {
            continue;
        }

        let dest = artwork_dir.join(format!("{}.png", sanitize_for_filename(&g.folder_path)));
        let artwork_path = resolve_game_artwork(
            &g.name,
            &g.folder_path,
            &g.exe_path,
            &dest,
            steamgriddb_key.as_deref(),
        )
        .map(|p| p.to_string_lossy().to_string());

        catalog.games.push(Game {
            available: std::path::Path::new(&g.exe_path).exists(),
            id: g.folder_path.clone(),
            name: g.name,
            folder_path: g.folder_path,
            exe_path: g.exe_path,
            artwork_path,
            has_custom_artwork: false,
        });
    }

    save_catalog(&app, &catalog)?;
    Ok(catalog)
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
    let artwork_dir = catalog_path(&app)?.parent().unwrap().join("artwork");
    let steamgriddb_key = std::env::var("STEAMGRIDDB_API_KEY")
        .ok()
        .filter(|key| scan::steamgriddb_reachable(key));

    for game in &mut catalog.games {
        if game.has_custom_artwork {
            continue;
        }
        let dest = artwork_dir.join(format!("{}.png", sanitize_for_filename(&game.id)));
        if let Some(path) = resolve_game_artwork(
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

    let dest = artwork_dir.join(format!("{}.png", sanitize_for_filename(&game.id)));
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
) -> Result<Catalog, String> {
    let mut catalog = load_catalog(&app)?;
    catalog.settings.root_folders = root_folders;
    catalog.settings.confirm_before_launch = confirm_before_launch;
    catalog.settings.show_output_log = show_output_log;
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
