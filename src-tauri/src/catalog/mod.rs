// Local JSON catalog: games, tag bindings, and settings. Single source of
// truth for what the gallery shows and what a tag insert launches.

use crate::launch::Store;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

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

/// Marks games whose exePath no longer exists on disk as unavailable (and
/// ones that reappeared as available again). Never adds, removes, or
/// otherwise touches `games` identity or `bindings` — availability is the
/// only thing a rescan is allowed to change.
pub fn rescan_availability(catalog: &mut Catalog) {
    for game in &mut catalog.games {
        game.available = Path::new(&game.exe_path).exists();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    fn temp_path(name: &str) -> std::path::PathBuf {
        env::temp_dir().join(format!("cart_reader_catalog_test_{}_{}.json", name, std::process::id()))
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
