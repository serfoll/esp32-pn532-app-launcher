// In-app ESP32 firmware flashing via the espflash library, so a reader
// with missing/wrong firmware can be fixed without leaving the app or
// reaching for Arduino IDE.

use espflash::connection::{Connection, ResetAfterOperation, ResetBeforeOperation};
use espflash::flasher::Flasher;
use espflash::target::DefaultProgressCallback;
use std::time::{Duration, Instant};

/// The exact firmware this app knows how to talk to (see
/// `sketch_jul20a.ino`'s `FIRMWARE_ID`) -- a single merged image
/// (bootloader + partition table + app) produced by
/// `arduino-cli compile --export-binaries`, flashed whole at offset 0
/// rather than as separate pieces at separate offsets. Baked into the app
/// itself rather than user-supplied, so there's no way to flash the wrong
/// thing onto the board.
const FIRMWARE: &[u8] = include_bytes!("../../firmware/RFIDCART_FW_v1.bin");
const FLASH_BAUD: u32 = 115_200;

// The caller sets the shared "flashing" flag before calling this, which
// stops the watchdog thread from *starting* any new port access -- but an
// already-in-flight probe_reader call can still be holding the port open
// for up to PROBE_TIMEOUT (800ms, see serial::mod) when the flag flips.
// Retrying past that window, rather than failing on the first busy port,
// avoids a flash that fails most of the time it's actually used (the
// Flash button only shows in ConnectedUnknownFirmware, where the
// watchdog's probe holds the port roughly half of every poll cycle).
const OPEN_RETRY_WINDOW: Duration = Duration::from_secs(3);
const OPEN_RETRY_INTERVAL: Duration = Duration::from_millis(200);

fn open_port_with_retry(port_name: &str, baud: u32) -> Result<serialport::COMPort, String> {
    let deadline = Instant::now() + OPEN_RETRY_WINDOW;
    let mut last_err = None;
    while Instant::now() < deadline {
        match serialport::new(port_name, baud).open_native() {
            Ok(port) => return Ok(port),
            Err(e) => last_err = Some(e),
        }
        std::thread::sleep(OPEN_RETRY_INTERVAL);
    }
    Err(format!(
        "couldn't open {port_name}: {}",
        last_err.map(|e| e.to_string()).unwrap_or_else(|| "timed out".to_string())
    ))
}

/// Flashes the bundled firmware to the board on `port_name`. Blocking and
/// can take anywhere from tens of seconds to a couple of minutes --
/// callers should run this off whatever thread is servicing the UI.
/// Board behavior during flashing (DTR/RTS bootloader entry, actual
/// flash timing) can only be verified against real hardware.
pub fn flash_firmware(port_name: &str) -> Result<(), String> {
    let usb_info = serialport::available_ports()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|p| p.port_name == port_name)
        .and_then(|p| match p.port_type {
            serialport::SerialPortType::UsbPort(info) => Some(info),
            _ => None,
        })
        .ok_or_else(|| format!("'{port_name}' is not a USB serial port"))?;

    let serial = open_port_with_retry(port_name, FLASH_BAUD)?;

    let connection = Connection::new(
        serial,
        usb_info,
        ResetAfterOperation::default(),
        ResetBeforeOperation::default(),
        FLASH_BAUD,
    );

    // use_stub=true (upload a small RAM stub for faster/more reliable
    // flashing), verify=true (read back and check after writing),
    // skip=false (always write, never skip based on a pre-check),
    // chip=None (auto-detect from the connected board rather than
    // assuming), baud=None (let espflash negotiate its own transfer
    // speed rather than pinning it to FLASH_BAUD).
    let mut flasher = Flasher::connect(connection, true, true, false, None, None)
        .map_err(|e| format!("couldn't connect to the board: {e}"))?;

    flasher
        .write_bin_to_flash(0, FIRMWARE, &mut DefaultProgressCallback)
        .map_err(|e| format!("flash failed: {e}"))
}
