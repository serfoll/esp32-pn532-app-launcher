// Local JSON catalog: games, tag bindings, and settings. Single source of
// truth for what the gallery shows and what a tag insert launches.

use crate::launch::Store;
use crate::scan;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn default_true() -> bool {
    true
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseBehavior {
    // Not decided yet -- ask again on the next close.
    #[default]
    Ask,
    Minimize,
    Quit,
}

// close_behavior used to be serialized as Option<bool> (None/true/false)
// before it became this enum. #[serde(default)] alone only covers a
// *missing* field; a catalog.json already on disk from before this change
// has the field *present* as `true`/`false`/`null`, which needs an explicit
// upgrade path here or it fails to parse at all -- confirmed live against a
// real catalog.json still carrying the old shape.
fn deserialize_close_behavior<'de, D>(deserializer: D) -> Result<CloseBehavior, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Legacy(Option<bool>),
        Current(CloseBehavior),
    }
    Ok(match Raw::deserialize(deserializer)? {
        Raw::Legacy(Some(true)) => CloseBehavior::Minimize,
        Raw::Legacy(Some(false)) => CloseBehavior::Quit,
        Raw::Legacy(None) => CloseBehavior::Ask,
        Raw::Current(behavior) => behavior,
    })
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub root_folders: Vec<String>,
    pub confirm_before_launch: bool,
    // #[serde(default)] so catalog.json files written before this field
    // existed still load (missing -> false) instead of failing to parse.
    #[serde(default)]
    pub show_output_log: bool,
    // #[serde(default)] keeps this Ask for catalog.json files written before
    // the close-behavior prompt existed at all (field missing entirely).
    #[serde(default, deserialize_with = "deserialize_close_behavior")]
    pub close_behavior: CloseBehavior,
    // #[serde(default = "default_true")] so catalog.json files written
    // before this field existed still load, defaulting to shown (matches
    // the toggle's own default) rather than serde's usual missing-bool-is-
    // false.
    #[serde(default = "default_true")]
    pub show_store_badges: bool,
    // #[serde(default = "default_true")] so catalog.json files written
    // before this setting existed still load with the new default-on
    // behavior, matching the toggle's own default, rather than serde's
    // usual missing-bool-is-false.
    #[serde(default = "default_true")]
    pub sync_on_startup: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub name: String,
    pub folder_path: String,
    pub exe_path: String,
    pub artwork_path: Option<String>,
    pub available: bool,
    // User-uploaded art should survive "Refresh artwork" instead of being
    // silently overwritten by the next auto-resolved (SteamGridDB/exe-icon)
    // result. #[serde(default)] keeps older catalog.json files loadable.
    #[serde(default)]
    pub has_custom_artwork: bool,
    // #[serde(default)] so catalog.json files written before this field
    // existed still load (missing -> None, no badge) instead of failing to
    // parse. Backfilled by the next "Refresh artwork" or rescan, same as
    // has_custom_artwork.
    #[serde(default)]
    pub store: Option<Store>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub tag_uid: String,
    pub game_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub version: u32,
    pub settings: Settings,
    pub games: Vec<Game>,
    pub bindings: Vec<Binding>,
}

impl Default for Catalog {
    fn default() -> Self {
        Catalog {
            version: 1,
            settings: Settings {
                root_folders: Vec::new(),
                confirm_before_launch: false,
                show_output_log: false,
                close_behavior: CloseBehavior::Ask,
                show_store_badges: true,
                sync_on_startup: true,
            },
            games: Vec::new(),
            bindings: Vec::new(),
        }
    }
}

/// Loads the catalog from `path`. A missing file means first launch and
/// returns the default (empty) catalog; a present-but-unparseable file is an
/// error to surface to the user, not something to silently reset.
pub fn load(path: &Path) -> io::Result<Catalog> {
    match fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Catalog::default()),
        Err(e) => Err(e),
    }
}

/// Writes the catalog atomically: serialize to a temp file next to `path`,
/// then rename over it. A crash mid-write leaves the old file intact rather
/// than a half-written catalog.json.
pub fn save(path: &Path, catalog: &Catalog) -> io::Result<()> {
    let tmp_path = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(catalog)?;
    fs::write(&tmp_path, json)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Marks a game unavailable unless its exe still exists on disk *and* its
/// folder still falls under one of the currently-tracked root folders (and
/// marks it available again once both hold). Never adds, removes, or
/// otherwise touches `games` identity or `bindings` — availability is the
/// only thing a rescan is allowed to change.
///
/// The root-folder half matters because removing a root folder from
/// Settings doesn't delete anything on disk -- the exe can still
/// physically exist, so an exe-existence check alone never notices the
/// removal, and the game keeps looking like an active part of the library
/// on every subsequent catalog load (get_catalog calls this on every app
/// launch and refresh). Every legitimately-cataloged game's folder is
/// already a subfolder of the root it was scanned from, so this can't
/// false-negative a game that's still genuinely part of the library.
pub fn rescan_availability(catalog: &mut Catalog) {
    let root_folders: Vec<String> = catalog
        .settings
        .root_folders
        .iter()
        .map(|p| p.to_lowercase())
        .collect();

    for game in &mut catalog.games {
        let exe_exists = Path::new(&game.exe_path).exists();
        let folder_lower = game.folder_path.to_lowercase();
        let under_tracked_root = root_folders
            .iter()
            .any(|root| Path::new(&folder_lower).starts_with(Path::new(root)));
        game.available = exe_exists && under_tracked_root;
    }
}

/// Detects the storefront for any game that doesn't have one on record yet
/// -- covers games cataloged before store detection existed. Cheap enough
/// (one manifest-file read per game) to run on every catalog load rather
/// than requiring a manual "Refresh artwork" click first: without this, a
/// game added before this feature shipped would show no badge until the
/// user happened to hit refresh, which isn't something the UI hints at.
pub fn backfill_stores(catalog: &mut Catalog) {
    for game in &mut catalog.games {
        if game.store.is_none() {
            game.store = crate::launch::detect_store(Path::new(&game.folder_path));
        }
    }
}

/// Games use their folder path as `id` — it's already unique per game, so a
/// separate uuid dependency would just be more code for the same guarantee.
pub fn sanitize_for_filename(id: &str) -> String {
    use std::hash::{Hash, Hasher};

    let readable: String = id
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();

    // The character-substitution above isn't injective -- "A-B" and "A_B"
    // both sanitize to "A_B", so two different games could collide onto
    // the same artwork file and silently overwrite each other's art. The
    // hash suffix (of the real, un-substituted id) makes the full result
    // collision-resistant while keeping the readable prefix for anyone
    // browsing the artwork folder by hand.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    format!("{readable}_{:x}", hasher.finish())
}

/// Resolves and writes artwork for one game: SteamGridDB first when a
/// (reachability-checked) key is available, local folder art / exe icon
/// otherwise or on any SteamGridDB failure. Shared by `add_confirmed_games`
/// and `refresh_all_artwork` so the fallback chain only lives in one place.
pub fn resolve_game_artwork(
    name: &str,
    folder_path: &str,
    exe_path: &str,
    dest: &Path,
    steamgriddb_key: Option<&str>,
) -> Option<PathBuf> {
    steamgriddb_key
        .and_then(|key| scan::fetch_steamgriddb_grid(name, key, dest))
        .or_else(|| scan::resolve_artwork(Path::new(folder_path), Some(Path::new(exe_path)), dest))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedGame {
    pub folder_path: String,
    pub name: String,
    pub exe_path: String,
}

/// Adds scanned candidates to the catalog, resolving artwork and store for
/// each. Shared by `confirm_games` (one folder, called right after the user
/// points the app at it) and `sync_library` (every already-registered
/// folder, called from the gallery's Sync button) so the add-a-game logic
/// only lives in one place. Candidates already in the catalog (same folder
/// path) are skipped rather than duplicated.
pub fn add_confirmed_games(
    catalog: &mut Catalog,
    artwork_dir: &Path,
    games: Vec<ConfirmedGame>,
    steamgriddb_key: Option<&str>,
) {
    for g in games {
        if catalog
            .games
            .iter()
            .any(|existing| existing.folder_path == g.folder_path)
        {
            continue;
        }

        let dest = artwork_dir.join(format!("{}.png", sanitize_for_filename(&g.folder_path)));
        let artwork_path =
            resolve_game_artwork(&g.name, &g.folder_path, &g.exe_path, &dest, steamgriddb_key)
                .map(|p| p.to_string_lossy().to_string());

        let store = crate::launch::detect_store(Path::new(&g.folder_path));

        catalog.games.push(Game {
            available: Path::new(&g.exe_path).exists(),
            id: g.folder_path.clone(),
            name: g.name,
            folder_path: g.folder_path,
            exe_path: g.exe_path,
            artwork_path,
            has_custom_artwork: false,
            store,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    fn temp_path(name: &str) -> std::path::PathBuf {
        env::temp_dir().join(format!("cart_reader_catalog_test_{}_{}.json", name, std::process::id()))
    }

    #[test]
    fn sanitize_for_filename_does_not_collide_on_ids_that_share_a_naive_sanitization() {
        // Both naively sanitize to "C__Games_A_B" (":" "-" "\\" all -> "_"),
        // which is exactly the collision that let one game's artwork
        // silently overwrite another's.
        let a = sanitize_for_filename("C:\\Games\\A-B");
        let b = sanitize_for_filename("C:\\Games\\A_B");
        assert_ne!(a, b, "distinct ids must not sanitize to the same filename");
    }

    fn sample_game(exe_path: &str) -> Game {
        Game {
            id: "g1".into(),
            name: "Some Game".into(),
            folder_path: "C:\\Games\\SomeGame".into(),
            exe_path: exe_path.into(),
            artwork_path: None,
            available: true,
            has_custom_artwork: false,
            store: None,
        }
    }

    #[test]
    fn add_confirmed_games_skips_already_cataloged_folders_and_adds_new_ones() {
        let mut catalog = Catalog::default();
        catalog.games.push(sample_game("C:\\Games\\SomeGame\\Game.exe"));

        let games = vec![
            ConfirmedGame {
                folder_path: "C:\\Games\\SomeGame".into(),
                name: "Duplicate".into(),
                exe_path: "C:\\Games\\SomeGame\\Game.exe".into(),
            },
            ConfirmedGame {
                folder_path: "C:\\Games\\NewGame".into(),
                name: "New Game".into(),
                exe_path: "C:\\Games\\NewGame\\NewGame.exe".into(),
            },
        ];

        add_confirmed_games(&mut catalog, Path::new("C:\\nonexistent_artwork_dir"), games, None);

        assert_eq!(catalog.games.len(), 2, "the already-cataloged folder should be skipped");
        let added = catalog
            .games
            .iter()
            .find(|g| g.folder_path == "C:\\Games\\NewGame")
            .expect("the new game should have been added");
        assert_eq!(added.name, "New Game");
        assert!(!added.available, "exe doesn't actually exist on disk");
    }

    #[test]
    fn backfill_stores_detects_store_for_a_game_missing_one() {
        let root = env::temp_dir().join(format!("cart_reader_backfill_match_{}", std::process::id()));
        fs::remove_dir_all(&root).ok();
        let game_dir = root.join("steamapps").join("common").join("SomeGame");
        fs::create_dir_all(&game_dir).unwrap();
        fs::write(
            root.join("steamapps").join("appmanifest_12345.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"12345\"\n\t\"installdir\"\t\t\"SomeGame\"\n}\n",
        )
        .unwrap();

        let mut game = sample_game("Game.exe");
        game.folder_path = game_dir.to_str().unwrap().to_string();
        let mut catalog = Catalog::default();
        catalog.games.push(game);

        backfill_stores(&mut catalog);
        fs::remove_dir_all(&root).ok();

        assert_eq!(catalog.games[0].store, Some(crate::launch::Store::Steam));
    }

    #[test]
    fn rescan_availability_marks_a_game_unavailable_once_its_root_is_untracked() {
        // The exe genuinely still exists on disk -- removing a root folder
        // never deletes anything -- so an exe-existence check alone would
        // never notice this game's root was untracked. This is the exact
        // bug: without the root-folder half of the check, a removed
        // folder's games kept showing as available on every subsequent
        // catalog load.
        let exe_path = temp_path("rescan_untracked_root").with_extension("exe");
        fs::write(&exe_path, b"").unwrap();

        let mut game = sample_game(exe_path.to_str().unwrap());
        game.folder_path = exe_path.parent().unwrap().to_str().unwrap().to_string();
        let mut catalog = Catalog::default();
        catalog.games.push(game);
        // root_folders left empty: this game's folder isn't tracked by any
        // currently-configured root, same as right after its one root
        // folder was removed from Settings.

        rescan_availability(&mut catalog);
        fs::remove_file(&exe_path).ok();

        assert!(!catalog.games[0].available, "exe exists, but its folder isn't under any tracked root");
    }

    #[test]
    fn rescan_availability_keeps_a_game_available_under_its_tracked_root() {
        let exe_path = temp_path("rescan_tracked_root").with_extension("exe");
        fs::write(&exe_path, b"").unwrap();
        let folder = exe_path.parent().unwrap().to_str().unwrap().to_string();

        let mut game = sample_game(exe_path.to_str().unwrap());
        game.folder_path = folder.clone();
        let mut catalog = Catalog::default();
        catalog.games.push(game);
        catalog.settings.root_folders.push(folder);

        rescan_availability(&mut catalog);
        fs::remove_file(&exe_path).ok();

        assert!(catalog.games[0].available, "exe exists and its folder is under a tracked root");
    }

    #[test]
    fn backfill_stores_leaves_an_already_known_store_alone() {
        let mut game = sample_game("Game.exe");
        game.folder_path = "C:\\Games\\NotSteamAnymore".into();
        game.store = Some(crate::launch::Store::Steam);
        let mut catalog = Catalog::default();
        catalog.games.push(game);

        backfill_stores(&mut catalog);

        assert_eq!(catalog.games[0].store, Some(crate::launch::Store::Steam));
    }

    #[test]
    fn round_trips_through_save_and_load() {
        let path = temp_path("roundtrip");
        let mut catalog = Catalog::default();
        catalog.games.push(sample_game("C:\\Games\\SomeGame\\Game.exe"));
        catalog.bindings.push(Binding {
            tag_uid: "04A3B2C1".into(),
            game_id: "g1".into(),
        });

        save(&path, &catalog).expect("save should succeed");
        let loaded = load(&path).expect("load should succeed");
        fs::remove_file(&path).ok();

        assert_eq!(loaded, catalog);
    }

    #[test]
    fn save_leaves_no_leftover_temp_file() {
        let path = temp_path("atomic");
        save(&path, &Catalog::default()).expect("save should succeed");

        let tmp_path = path.with_extension("json.tmp");
        assert!(!tmp_path.exists(), "temp file should be renamed away, not left behind");

        fs::remove_file(&path).ok();
    }

    #[test]
    fn missing_file_loads_as_default() {
        let path = temp_path("missing");
        fs::remove_file(&path).ok(); // ensure it really doesn't exist

        let loaded = load(&path).expect("missing file should load as default, not error");
        assert_eq!(loaded, Catalog::default());
    }

    #[test]
    fn loads_pre_show_output_log_catalog_with_field_defaulted_false() {
        let path = temp_path("legacy_schema");
        fs::write(
            &path,
            r#"{"version":1,"settings":{"rootFolders":[],"confirmBeforeLaunch":false},"games":[],"bindings":[]}"#,
        )
        .unwrap();

        let loaded = load(&path).expect("catalog written before showOutputLog existed should still load");
        fs::remove_file(&path).ok();

        assert!(!loaded.settings.show_output_log);
    }

    #[test]
    fn loads_pre_close_behavior_catalog_with_field_defaulted_to_ask() {
        let path = temp_path("legacy_close_behavior");
        fs::write(
            &path,
            r#"{"version":1,"settings":{"rootFolders":[],"confirmBeforeLaunch":false,"showOutputLog":false},"games":[],"bindings":[]}"#,
        )
        .unwrap();

        let loaded = load(&path).expect("catalog written before closeBehavior existed should still load");
        fs::remove_file(&path).ok();

        assert_eq!(loaded.settings.close_behavior, CloseBehavior::Ask);
    }

    #[test]
    fn loads_bool_close_behavior_catalog_upgrading_to_the_enum() {
        let path = temp_path("legacy_bool_close_behavior");
        fs::write(
            &path,
            r#"{"version":1,"settings":{"rootFolders":[],"confirmBeforeLaunch":false,"showOutputLog":false,"closeBehavior":true},"games":[],"bindings":[]}"#,
        )
        .unwrap();

        let loaded = load(&path).expect("catalog with the old bool closeBehavior should still load");
        fs::remove_file(&path).ok();

        assert_eq!(loaded.settings.close_behavior, CloseBehavior::Minimize);
    }

    #[test]
    fn loads_null_close_behavior_catalog_upgrading_to_ask() {
        let path = temp_path("legacy_null_close_behavior");
        fs::write(
            &path,
            r#"{"version":1,"settings":{"rootFolders":[],"confirmBeforeLaunch":false,"showOutputLog":false,"closeBehavior":null},"games":[],"bindings":[]}"#,
        )
        .unwrap();

        let loaded = load(&path).expect("catalog with the old null closeBehavior should still load");
        fs::remove_file(&path).ok();

        assert_eq!(loaded.settings.close_behavior, CloseBehavior::Ask);
    }

    #[test]
    fn loads_pre_has_custom_artwork_catalog_with_field_defaulted_false() {
        let path = temp_path("legacy_game_schema");
        fs::write(
            &path,
            r#"{"version":1,"settings":{"rootFolders":[],"confirmBeforeLaunch":false,"showOutputLog":false},"games":[{"id":"g1","name":"Some Game","folderPath":"C:\\Games\\SomeGame","exePath":"C:\\Games\\SomeGame\\Game.exe","artworkPath":null,"available":true}],"bindings":[]}"#,
        )
        .unwrap();

        let loaded = load(&path).expect("catalog written before hasCustomArtwork existed should still load");
        fs::remove_file(&path).ok();

        assert!(!loaded.games[0].has_custom_artwork);
    }

    #[test]
    fn loads_pre_store_catalog_with_game_store_defaulted_none_and_setting_defaulted_true() {
        let path = temp_path("legacy_store_schema");
        fs::write(
            &path,
            r#"{"version":1,"settings":{"rootFolders":[],"confirmBeforeLaunch":false,"showOutputLog":false},"games":[{"id":"g1","name":"Some Game","folderPath":"C:\\Games\\SomeGame","exePath":"C:\\Games\\SomeGame\\Game.exe","artworkPath":null,"available":true,"hasCustomArtwork":false}],"bindings":[]}"#,
        )
        .unwrap();

        let loaded = load(&path).expect("catalog written before store existed should still load");
        fs::remove_file(&path).ok();

        assert_eq!(loaded.games[0].store, None);
        assert!(loaded.settings.show_store_badges);
    }

    #[test]
    fn loads_pre_sync_on_startup_catalog_with_setting_defaulted_true() {
        let path = temp_path("legacy_sync_on_startup_schema");
        fs::write(
            &path,
            r#"{"version":1,"settings":{"rootFolders":[],"confirmBeforeLaunch":false,"showOutputLog":false,"showStoreBadges":true},"games":[],"bindings":[]}"#,
        )
        .unwrap();

        let loaded = load(&path).expect("catalog written before syncOnStartup existed should still load");
        fs::remove_file(&path).ok();

        assert!(loaded.settings.sync_on_startup);
    }

    #[test]
    fn corrupt_file_errors_instead_of_silently_resetting() {
        let path = temp_path("corrupt");
        fs::write(&path, "{ not valid json").unwrap();

        let result = load(&path);
        fs::remove_file(&path).ok();

        assert!(result.is_err(), "corrupt catalog should surface as an error, not reset silently");
    }

    #[test]
    fn rescan_updates_availability_without_touching_bindings() {
        let missing_exe = temp_path("does-not-exist").with_extension("exe");
        let mut catalog = Catalog::default();
        catalog.games.push(sample_game(missing_exe.to_str().unwrap()));
        catalog.bindings.push(Binding {
            tag_uid: "04A3B2C1".into(),
            game_id: "g1".into(),
        });
        let bindings_before = catalog.bindings.clone();

        rescan_availability(&mut catalog);

        assert_eq!(catalog.games.len(), 1, "rescan must not add or remove games");
        assert!(!catalog.games[0].available, "game with a missing exe should be marked unavailable");
        assert_eq!(catalog.bindings, bindings_before, "rescan must never mutate bindings");
    }
}
