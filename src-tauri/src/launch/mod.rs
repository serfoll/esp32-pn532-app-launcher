// Platform-aware game launching: prefers a launcher's own protocol handler
// (currently Steam) over spawning the exe directly, since many
// launcher-installed games are DRM-wrapped stubs that exit silently when
// run outside their normal launcher instead of showing an error.

use std::fs;
use std::path::Path;

/// Extracts one `"key"    "value"` pair from Valve's ACF text format.
/// Whitespace between key and value varies (tabs in real files, spaces in
/// tests), so this splits on the quotes themselves rather than assuming a
/// fixed separator.
fn acf_value(contents: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    for line in contents.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(&needle) {
            let value_start = rest.find('"')? + 1;
            let value_end = rest[value_start..].find('"')? + value_start;
            return Some(rest[value_start..value_end].to_string());
        }
    }
    None
}

/// Finds the Steam App ID for a game installed at `folder_path`, by reading
/// the `appmanifest_<id>.acf` file Steam writes one level above
/// `steamapps/common/<game>/` and matching its `installdir` back to this
/// folder's name. Returns `None` for anything not installed through Steam.
pub fn find_steam_app_id(folder_path: &Path) -> Option<String> {
    let steamapps_dir = folder_path.parent()?.parent()?;
    let install_dir_name = folder_path.file_name()?.to_str()?;

    for entry in fs::read_dir(steamapps_dir).ok()?.flatten() {
        let path = entry.path();
        let name = path.file_name()?.to_str()?.to_string();
        if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
            continue;
        }
        let contents = fs::read_to_string(&path).ok()?;
        if acf_value(&contents, "installdir").as_deref() == Some(install_dir_name) {
            return acf_value(&contents, "appid");
        }
    }
    None
}

/// Checks whether any currently-running process's executable lives
/// somewhere under `folder_path`, so a cart pulled and reinserted while its
/// game is still open doesn't spawn a second instance.
///
/// Deliberately checks the whole install folder rather than the one exe we
/// recorded: some launchers (EA App, confirmed live) hand off from a thin
/// stub exe to a *different* exe in the same folder, then exit the stub by
/// design. An exact-exe check would see the stub gone the moment it hands
/// off and wrongly conclude "not running" while the real game keeps going
/// right next to it.
///
/// Uses `Path::starts_with` (component-wise) rather than a raw string
/// prefix, so a folder like "EA SPORTS FC 24" doesn't false-match a
/// sibling "EA SPORTS FC 24 Beta". Compares case-insensitively: Windows
/// paths are case-preserving but case-insensitive, and in practice
/// `std::env::current_exe()` and sysinfo's reported process path can
/// differ only in case for the exact same file (observed directly: one
/// returns `_Dev`, the other `_DEV` for this project's own path).
pub fn is_game_running(folder_path: &str) -> bool {
    let target = Path::new(&folder_path.to_lowercase()).to_path_buf();
    let mut system = sysinfo::System::new_all();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    system.processes().values().any(|p| {
        p.exe().is_some_and(|exe| {
            Path::new(&exe.to_string_lossy().to_lowercase()).starts_with(&target)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_root(name: &str) -> std::path::PathBuf {
        let dir = env::temp_dir().join(format!("cart_reader_launch_test_{}_{}", name, std::process::id()));
        fs::remove_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn finds_app_id_matching_installdir() {
        let root = temp_root("match");
        let steamapps = root.join("steamapps");
        let game_dir = steamapps.join("common").join("SomeGame");
        fs::create_dir_all(&game_dir).unwrap();
        fs::write(
            steamapps.join("appmanifest_12345.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"12345\"\n\t\"installdir\"\t\t\"SomeGame\"\n}\n",
        )
        .unwrap();

        let result = find_steam_app_id(&game_dir);
        fs::remove_dir_all(&root).ok();

        assert_eq!(result, Some("12345".to_string()));
    }

    #[test]
    fn ignores_manifest_for_a_different_installdir() {
        let root = temp_root("mismatch");
        let steamapps = root.join("steamapps");
        let game_dir = steamapps.join("common").join("SomeGame");
        fs::create_dir_all(&game_dir).unwrap();
        fs::write(
            steamapps.join("appmanifest_999.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"999\"\n\t\"installdir\"\t\t\"OtherGame\"\n}\n",
        )
        .unwrap();

        let result = find_steam_app_id(&game_dir);
        fs::remove_dir_all(&root).ok();

        assert_eq!(result, None);
    }

    #[test]
    fn returns_none_when_not_a_steam_install() {
        let root = temp_root("not_steam");
        let game_dir = root.join("C_Games").join("SomeGame");
        fs::create_dir_all(&game_dir).unwrap();

        let result = find_steam_app_id(&game_dir);
        fs::remove_dir_all(&root).ok();

        assert_eq!(result, None);
    }

    #[test]
    fn detects_the_current_process_by_its_folder_not_its_exact_exe_name() {
        // Mirrors the EA App case: checking the *folder* still finds the
        // running process even without matching its exact exe filename --
        // the whole point, since a stub launcher's recorded exe often isn't
        // the one that ends up actually running.
        let current_exe = std::env::current_exe().unwrap();
        let folder = current_exe.parent().unwrap().to_str().unwrap();
        assert!(is_game_running(folder));
    }

    #[test]
    fn does_not_false_match_a_truncated_folder_that_only_shares_a_string_prefix() {
        // A naive string-prefix check would wrongly match a chopped-up
        // path like ".../exam" against the real ".../examples/..." --
        // component-wise Path::starts_with must not.
        let current_exe = std::env::current_exe().unwrap();
        let folder = current_exe.parent().unwrap().to_str().unwrap().to_string();
        let truncated = &folder[..folder.len() - 3];
        assert!(!is_game_running(truncated));
    }

    #[test]
    fn returns_false_for_a_folder_nothing_is_running_from() {
        assert!(!is_game_running(r"C:\definitely\not\a\real\running\folder"));
    }
}
