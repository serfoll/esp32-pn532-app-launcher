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
}
