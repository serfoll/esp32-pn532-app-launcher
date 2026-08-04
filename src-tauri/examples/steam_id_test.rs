// Throwaway manual-verification harness for Steam App ID detection against
// a real installed library. Not part of the app.
//   cargo run --example steam_id_test --manifest-path src-tauri/Cargo.toml -- "D:\SteamLibrary\steamapps\common\Sekiro"

fn main() {
    let path = std::env::args().nth(1).expect("pass a game folder path");
    match cart_reader_lib::launch::find_steam_app_id(std::path::Path::new(&path)) {
        Some(id) => println!("app id: {id} -> steam://rungameid/{id}"),
        None => println!("no Steam App ID found for {path}"),
    }
}
