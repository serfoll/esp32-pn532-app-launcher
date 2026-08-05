// Folder scanning + best-guess main-exe detection, plus artwork resolution
// (SteamGridDB when online and configured, local folder art / exe icon as
// the offline fallback). No Tauri/UI concerns here.

use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const STEAMGRIDDB_TIMEOUT: Duration = Duration::from_secs(5);

pub struct ScanCandidate {
    pub folder_path: String,
    pub name: String,
    pub exe_path: Option<String>,
}

const IGNORED_EXE_SUBSTRINGS: &[&str] = &[
    "unins",
    "uninstall",
    "setup",
    "redist",
    "vcredist",
    "vc_redist",
    "directx",
    "dxsetup",
    "dxwebsetup",
    "crashreporter",
    "crashhandler",
    "dotnet",
];

fn is_plausible_game_exe(path: &Path) -> bool {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    !IGNORED_EXE_SUBSTRINGS.iter().any(|s| name.contains(s))
}

/// Recursively collects .exe files under `dir`, skipping obvious
/// installer/uninstaller/redist noise. Depth is bounded so a deeply nested
/// SDK or asset folder doesn't turn into a multi-minute walk.
fn find_exe_candidates(dir: &Path, depth: u8) -> Vec<PathBuf> {
    const MAX_DEPTH: u8 = 6;
    let mut found = Vec::new();
    if depth > MAX_DEPTH {
        return found;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(find_exe_candidates(&path, depth + 1));
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            && is_plausible_game_exe(&path)
        {
            found.push(path);
        }
    }
    found
}

/// Scores a candidate exe against the folder's name: exact stem match
/// beats substring match beats no match, with file size as the tiebreak.
///
/// ponytail: tie-break order is a best guess, not validated against a real
/// library — spec Open Question 3 calls for a manual survey against the
/// user's actual C:\Games before locking this ranking in.
fn score_candidate(folder_name: &str, exe: &Path) -> (u8, u64) {
    let stem = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let folder_name_lower = folder_name.to_lowercase();

    let name_score: u8 = if stem == folder_name_lower {
        2
    } else if stem.contains(&folder_name_lower) || folder_name_lower.contains(&stem) {
        1
    } else {
        0
    };

    let size = fs::metadata(exe).map(|m| m.len()).unwrap_or(0);
    (name_score, size)
}

/// Scans each immediate subfolder of `root` for a plausible main exe.
/// Subfolders where nothing plausible is found are still returned with
/// `exe_path: None` — flagged, not silently dropped, per the spec's
/// Success Criteria ("any folder where no plausible exe is found is
/// flagged, not silently dropped").
pub fn scan_root(root: &Path) -> Vec<ScanCandidate> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let best = find_exe_candidates(&path, 0)
            .into_iter()
            .max_by_key(|exe| score_candidate(&name, exe));

        candidates.push(ScanCandidate {
            folder_path: path.to_string_lossy().to_string(),
            name,
            exe_path: best.map(|p| p.to_string_lossy().to_string()),
        });
    }

    candidates
}

const FOLDER_ART_CANDIDATES: &[&str] = &[
    "folder.png",
    "folder.jpg",
    "folder.jpeg",
    "cover.png",
    "cover.jpg",
    "cover.jpeg",
    "box.png",
    "box.jpg",
    "icon.png",
    "icon.jpg",
];

/// Looks for a conventionally-named art file directly inside `folder`
/// (folder.png, cover.jpg, box.png, ...) — the common places games/launchers
/// drop cover art. First match wins; MVP doesn't rank multiple matches.
fn find_folder_art(folder: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(folder).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if FOLDER_ART_CANDIDATES.contains(&name.to_lowercase().as_str()) {
            return Some(path);
        }
    }
    None
}

/// Extracts `exe_path`'s embedded icon and writes it as a PNG to `dest`.
fn extract_exe_icon(exe_path: &Path, dest: &Path) -> Result<(), String> {
    let path_str = exe_path.to_str().ok_or("exe path is not valid UTF-8")?;
    let icon = windows_icons::get_icon_by_path(path_str).map_err(|e| format!("{e:?}"))?;
    icon.save(dest).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

fn ensure_parent_dir(dest: &Path) -> Option<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).ok()?;
    }
    Some(())
}

/// Resolves the artwork for a game: folder art wins if present (a human
/// picked it, it's usually nicer than an exe icon), otherwise falls back to
/// the exe's embedded icon. Writes the result to `dest` and returns it, or
/// `None` if neither source is available. This is the fully-offline path —
/// see `fetch_steamgriddb_grid` for the opportunistic online lookup that
/// runs before this as a first choice, when configured.
pub fn resolve_artwork(folder: &Path, exe_path: Option<&Path>, dest: &Path) -> Option<PathBuf> {
    ensure_parent_dir(dest)?;

    if let Some(art) = find_folder_art(folder) {
        fs::copy(&art, dest).ok()?;
        return Some(dest.to_path_buf());
    }

    extract_exe_icon(exe_path?, dest).ok()?;
    Some(dest.to_path_buf())
}

#[derive(Deserialize)]
struct SteamGridDbSearchResult {
    id: u64,
}

#[derive(Deserialize)]
struct SteamGridDbSearchResponse {
    data: Vec<SteamGridDbSearchResult>,
}

#[derive(Deserialize)]
struct SteamGridDbGrid {
    url: String,
}

#[derive(Deserialize)]
struct SteamGridDbGridsResponse {
    data: Vec<SteamGridDbGrid>,
}

/// Looks `name` up on SteamGridDB and downloads its top portrait grid (the
/// tall library-capsule art, not the square icon) to `dest`. Requests
/// `dimensions=600x900` specifically so results are the portrait style,
/// not SteamGridDB's wide 460x215 grids — the gallery displays these at a
/// 9:16 box via `object-fit: cover`, and cropping a wide grid down to that
/// would cut off far more of the art than cropping a portrait one.
/// Returns `None` on any failure along the way (offline, bad/missing key,
/// no match, request error) so the caller can fall back to local artwork
/// sources — this is an opportunistic enhancement, not a required step, and
/// a slow/unreachable network must never block adding a game.
pub fn fetch_steamgriddb_grid(name: &str, api_key: &str, dest: &Path) -> Option<PathBuf> {
    let client = reqwest::blocking::Client::builder()
        .timeout(STEAMGRIDDB_TIMEOUT)
        .build()
        .ok()?;

    let mut search_url =
        reqwest::Url::parse("https://www.steamgriddb.com/api/v2/search/autocomplete").ok()?;
    search_url.path_segments_mut().ok()?.push(name);
    let search: SteamGridDbSearchResponse = client
        .get(search_url)
        .bearer_auth(api_key)
        .send()
        .ok()?
        .json()
        .ok()?;
    let game_id = search.data.first()?.id;

    let grids: SteamGridDbGridsResponse = client
        .get(format!(
            "https://www.steamgriddb.com/api/v2/grids/game/{game_id}?dimensions=600x900"
        ))
        .bearer_auth(api_key)
        .send()
        .ok()?
        .json()
        .ok()?;
    let grid_url = &grids.data.first()?.url;

    let bytes = client.get(grid_url).send().ok()?.bytes().ok()?;
    let image = image::load_from_memory(&bytes).ok()?;
    ensure_parent_dir(dest)?;
    image.save(dest).ok()?;
    Some(dest.to_path_buf())
}

/// Cheap upfront reachability check for SteamGridDB. `confirm_games` can
/// process a whole batch of scanned folders at once; without this, a
/// reachable-but-slow-to-fail network would pay `fetch_steamgriddb_grid`'s
/// full per-request timeout up to three times *per game* in the batch. One
/// short check up front bounds the offline/unreachable case to a couple of
/// seconds total instead of compounding across every game.
pub fn steamgriddb_reachable(api_key: &str) -> bool {
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    client
        .get("https://www.steamgriddb.com/api/v2/search/autocomplete/a")
        .bearer_auth(api_key)
        .send()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_root(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "cart_reader_scan_test_{}_{}",
            name,
            std::process::id()
        ));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(path: &Path, size: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; size]).unwrap();
    }

    #[test]
    fn n_subfolders_produce_n_candidates() {
        let root = temp_root("count");
        write_file(&root.join("GameA").join("GameA.exe"), 10);
        write_file(&root.join("GameB").join("GameB.exe"), 10);
        write_file(&root.join("GameC").join("GameC.exe"), 10);

        let candidates = scan_root(&root);
        fs::remove_dir_all(&root).ok();

        assert_eq!(candidates.len(), 3);
    }

    #[test]
    fn picks_exe_matching_folder_name_over_larger_unrelated_exe() {
        let root = temp_root("name_match");
        write_file(&root.join("SomeGame").join("SomeGame.exe"), 10);
        write_file(&root.join("SomeGame").join("bin").join("helper_tool.exe"), 999_999);

        let candidates = scan_root(&root);
        fs::remove_dir_all(&root).ok();

        assert_eq!(candidates.len(), 1);
        assert!(candidates[0]
            .exe_path
            .as_ref()
            .unwrap()
            .ends_with("SomeGame.exe"));
    }

    #[test]
    fn folder_with_no_exe_is_flagged_not_dropped() {
        let root = temp_root("no_exe");
        fs::create_dir_all(root.join("JustDataFiles")).unwrap();
        write_file(&root.join("JustDataFiles").join("readme.txt"), 5);

        let candidates = scan_root(&root);
        fs::remove_dir_all(&root).ok();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "JustDataFiles");
        assert!(candidates[0].exe_path.is_none());
    }

    #[test]
    fn folder_art_wins_over_exe_icon() {
        let root = temp_root("folder_art");
        write_file(&root.join("folder.png"), 20);
        let dest = root.join("artwork.png");

        let result = resolve_artwork(&root, None, &dest);
        fs::remove_dir_all(&root).ok();

        assert_eq!(result, Some(dest));
    }

    #[test]
    fn falls_back_to_exe_icon_when_no_folder_art() {
        let root = temp_root("exe_icon_fallback");
        let dest = root.join("artwork.png");
        let notepad = Path::new(r"C:\Windows\System32\notepad.exe");
        assert!(notepad.exists(), "test assumes notepad.exe exists on this Windows machine");

        let result = resolve_artwork(&root, Some(notepad), &dest);
        let extracted = dest.exists();
        fs::remove_dir_all(&root).ok();

        assert_eq!(result, Some(dest));
        assert!(extracted, "expected a PNG to be written from notepad.exe's icon");
    }

    #[test]
    fn returns_none_with_no_folder_art_and_no_exe() {
        let root = temp_root("no_art_no_exe");
        let dest = root.join("artwork.png");

        let result = resolve_artwork(&root, None, &dest);
        fs::remove_dir_all(&root).ok();

        assert_eq!(result, None);
    }

    #[test]
    fn ignores_installer_and_redist_exes() {
        let root = temp_root("ignore_installers");
        write_file(&root.join("SomeGame").join("unins000.exe"), 999_999);
        write_file(&root.join("SomeGame").join("vc_redist.x64.exe"), 999_999);
        write_file(&root.join("SomeGame").join("SomeGame.exe"), 10);

        let candidates = scan_root(&root);
        fs::remove_dir_all(&root).ok();

        assert_eq!(candidates.len(), 1);
        assert!(candidates[0]
            .exe_path
            .as_ref()
            .unwrap()
            .ends_with("SomeGame.exe"));
    }
}
