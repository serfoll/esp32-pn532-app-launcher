// Platform-aware game launching: prefers a launcher's own protocol handler
// (currently Steam) over spawning the exe directly, since many
// launcher-installed games are DRM-wrapped stubs that exit silently when
// run outside their normal launcher instead of showing an error.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Storefront a game was installed through, detected at scan time and shown
/// as a badge on its gallery card. Steam-only for now -- `detect_store` is
/// the one place a second store's detector would plug in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Store {
    Steam,
}

/// Tries each known store detector against `folder_path`, in order.
pub fn detect_store(folder_path: &Path) -> Option<Store> {
    find_steam_app_id(folder_path).map(|_| Store::Steam)
}

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
    running_folders([folder_path]).contains(folder_path)
}

/// The one folder-match rule `running_folders` and `stop_game` both use, so
/// "is this game running" and "stop this game" can never disagree about
/// which processes count as belonging to it.
fn exe_belongs_to_folder(exe_lowercase: &str, folder_lowercase_target: &Path) -> bool {
    Path::new(exe_lowercase).starts_with(folder_lowercase_target)
}

/// Snapshots every currently-running process. Shared by `running_folders`
/// and `stop_game` so a fresh `System::new_all()` + `refresh_processes()`
/// (the expensive part of a scan) only happens in one place.
fn refreshed_system() -> sysinfo::System {
    let mut system = sysinfo::System::new_all();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    system
}

/// Same folder-prefix check as `is_game_running`, batched across every
/// folder passed in against a single process-list scan -- badging every
/// game's running state on each poll tick would otherwise re-scan all
/// system processes once per game instead of once total.
pub fn running_folders<'a, I>(folder_paths: I) -> std::collections::HashSet<String>
where
    I: IntoIterator<Item = &'a str>,
{
    let system = refreshed_system();
    let running_exes: Vec<String> = system
        .processes()
        .values()
        .filter_map(|p| p.exe())
        .map(|exe| exe.to_string_lossy().to_lowercase())
        .collect();

    folder_paths
        .into_iter()
        .filter(|folder_path| {
            let target = Path::new(&folder_path.to_lowercase()).to_path_buf();
            running_exes.iter().any(|exe| exe_belongs_to_folder(exe, &target))
        })
        .map(|folder_path| folder_path.to_string())
        .collect()
}

/// Kills every currently-running process whose exe lives under
/// `folder_path` -- the gallery's "Stop" action, the inverse of launching.
/// Returns how many processes were actually killed (0 if the game wasn't
/// running, which isn't an error -- it can legitimately race the next poll
/// tick noticing the game already exited on its own).
pub fn stop_game(folder_path: &str) -> usize {
    let system = refreshed_system();
    let target = Path::new(&folder_path.to_lowercase()).to_path_buf();

    let mut killed = 0;
    for process in system.processes().values() {
        let Some(exe) = process.exe() else { continue };
        let exe_lower = exe.to_string_lossy().to_lowercase();
        if exe_belongs_to_folder(&exe_lower, &target) && process.kill() {
            killed += 1;
        }
    }
    killed
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
    fn detect_store_maps_a_steam_install_to_store_steam() {
        let root = temp_root("detect_store_match");
        let steamapps = root.join("steamapps");
        let game_dir = steamapps.join("common").join("SomeGame");
        fs::create_dir_all(&game_dir).unwrap();
        fs::write(
            steamapps.join("appmanifest_12345.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"12345\"\n\t\"installdir\"\t\t\"SomeGame\"\n}\n",
        )
        .unwrap();

        let result = detect_store(&game_dir);
        fs::remove_dir_all(&root).ok();

        assert_eq!(result, Some(Store::Steam));
    }

    #[test]
    fn detect_store_returns_none_for_a_non_steam_install() {
        let root = temp_root("detect_store_none");
        let game_dir = root.join("C_Games").join("SomeGame");
        fs::create_dir_all(&game_dir).unwrap();

        let result = detect_store(&game_dir);
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

    #[test]
    fn running_folders_finds_the_current_process_among_several_checked_at_once() {
        let current_exe = std::env::current_exe().unwrap();
        let folder = current_exe.parent().unwrap().to_str().unwrap();
        let not_running = r"C:\definitely\not\a\real\running\folder";

        let running = running_folders([not_running, folder]);

        assert_eq!(running.len(), 1);
        assert!(running.contains(folder));
    }

    #[test]
    fn stop_game_kills_a_process_running_from_the_target_folder() {
        // Runs a copy of cmd.exe from an isolated temp folder rather than
        // targeting the real C:\Windows\System32 -- stop_game kills every
        // matching process under the folder it's given, and System32 hosts
        // dozens of unrelated live system processes.
        let root = temp_root("stop");
        fs::create_dir_all(&root).unwrap();
        let cmd_copy = root.join("cmd.exe");
        fs::copy(r"C:\Windows\System32\cmd.exe", &cmd_copy)
            .expect("failed to copy cmd.exe for the test");

        // No /C command -- a bare cmd.exe just sits at its prompt reading a
        // line from stdin. A piped (not inherited) stdin that's never
        // closed keeps it blocked indefinitely without needing a real
        // console, which `pause`/`timeout` require and a test harness
        // doesn't provide.
        let mut child = std::process::Command::new(&cmd_copy)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed to spawn test process");
        std::thread::sleep(std::time::Duration::from_millis(300));

        let killed = stop_game(root.to_str().unwrap());
        let _ = child.wait();
        fs::remove_dir_all(&root).ok();

        assert!(killed >= 1, "expected stop_game to kill the copied cmd.exe");
    }
}
