// Throwaway manual-verification harness for the SteamGridDB lookup against
// the real API with a real key from .env.local/.env. Not part of the app.
//   cargo run --example steamgriddb_test --manifest-path src-tauri/Cargo.toml -- "Sekiro"

fn main() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let _ = dotenvy::from_path(repo_root.join(".env.local"))
        .or_else(|_| dotenvy::from_path(repo_root.join(".env")));

    let Ok(key) = std::env::var("STEAMGRIDDB_API_KEY") else {
        println!("STEAMGRIDDB_API_KEY not set in .env.local or .env");
        return;
    };

    let name = std::env::args().nth(1).unwrap_or_else(|| "Sekiro".to_string());
    let dest = std::env::temp_dir().join("steamgriddb_test_grid.png");

    match cart_reader_lib::scan::fetch_steamgriddb_grid(&name, &key, &dest) {
        Some(path) => println!("ok: saved grid for '{name}' to {}", path.display()),
        None => println!("failed: no grid found (or a request/network error) for '{name}'"),
    }
}
