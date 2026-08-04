// Throwaway manual-verification harness for the serial watchdog against
// real hardware. Not part of the app — run with:
//   cargo run --example probe_reader --manifest-path src-tauri/Cargo.toml

fn main() {
    match cart_reader_lib::serial::find_reader_port() {
        Some(port) => {
            println!("found port: {port}");
            let state = cart_reader_lib::serial::probe_reader(&port, std::time::Duration::from_millis(800));
            println!("state: {state:?}");
        }
        None => println!("no USB serial port found"),
    }
}
