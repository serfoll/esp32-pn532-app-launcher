// Throwaway manual-verification harness for the exe-detection heuristic
// against a real game library. Not part of the app — run with:
//   cargo run --example scan_survey --manifest-path src-tauri/Cargo.toml -- "C:\Games"

fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| "C:\\Games".to_string());
    let candidates = cart_reader_lib::scan::scan_root(std::path::Path::new(&root));

    if candidates.is_empty() {
        println!("no subfolders found under {root}");
        return;
    }

    for c in &candidates {
        match &c.exe_path {
            Some(exe) => println!("{:<30} -> {exe}", c.name),
            None => println!("{:<30} -> (no plausible exe found)", c.name),
        }
    }
}
