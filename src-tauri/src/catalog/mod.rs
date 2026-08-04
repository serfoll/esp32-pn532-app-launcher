// Local JSON catalog: games, tag bindings, and settings. Single source of
// truth for what the gallery shows and what a tag insert launches.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub root_folders: Vec<String>,
    pub confirm_before_launch: bool,
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
        }
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
