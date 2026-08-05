// In-app ESP32 firmware flashing via the espflash library, so a reader
// with missing/wrong firmware can be fixed without leaving the app or
// reaching for Arduino IDE.

use espflash::connection::{Connection, ResetAfterOperation, ResetBeforeOperation};
use espflash::flasher::Flasher;
use espflash::target::ProgressCallbacks;
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

// Seed default: the reader's USB-serial chip (a CH340), confirmed live via
// Windows Device Manager. Used until the user pairs a specific device via
// the pairing dialog (Settings.readerUsbVid/Pid) -- see
// commands::flash_firmware for how the two combine. `find_reader_port`
// (serial::mod) deliberately has no VID/PID allowlist at all -- reading
// garbage from the wrong device is harmless, so that path stays true to
// this app's "personal, single-board use" design. Flashing is a different
// risk: writing firmware to the wrong device could silently overwrite
// something unrelated, so this check is scoped to flash_firmware
// specifically rather than loosening the shared port-detection helper
// both paths would otherwise have to agree on.
pub const DEFAULT_READER_USB_VID: u16 = 0x1A86;
pub const DEFAULT_READER_USB_PID: u16 = 0x7523;

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

fn is_reader_usb_device(vid: u16, pid: u16, expected_vid: u16, expected_pid: u16) -> bool {
    vid == expected_vid && pid == expected_pid
}

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

/// Flashes the bundled firmware to the board on `port_name`, reporting
/// progress through `progress` (init/update/verifying/finish -- see
/// `espflash::target::ProgressCallbacks`). `expected_vid`/`expected_pid`
/// gate which USB device is allowed to receive the flash -- callers
/// resolve these from `Settings.readerUsbVid`/`Pid` once paired, falling
/// back to `DEFAULT_READER_USB_VID`/`PID` until then. Blocking and can
/// take anywhere from tens of seconds to a couple of minutes -- callers
/// must run this off the main thread themselves (Tauri runs plain `fn`
/// commands on the main thread by default, which would otherwise freeze
/// the whole window for the entire flash). Board behavior during flashing
/// (DTR/RTS bootloader entry, actual flash timing) can only be verified
/// against real hardware.
pub fn flash_firmware(
    port_name: &str,
    expected_vid: u16,
    expected_pid: u16,
    progress: &mut dyn ProgressCallbacks,
) -> Result<(), String> {
    let usb_info = serialport::available_ports()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|p| p.port_name == port_name)
        .and_then(|p| match p.port_type {
            serialport::SerialPortType::UsbPort(info) => Some(info),
            _ => None,
        })
        .ok_or_else(|| format!("'{port_name}' is not a USB serial port"))?;

    if !is_reader_usb_device(usb_info.vid, usb_info.pid, expected_vid, expected_pid) {
        return Err(format!(
            "'{port_name}' doesn't look like the reader (USB {:04X}:{:04X}, expected {expected_vid:04X}:{expected_pid:04X})",
            usb_info.vid, usb_info.pid
        ));
    }

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
        .write_bin_to_flash(0, FIRMWARE, progress)
        .map_err(|e| format!("flash failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_reader_usb_device_matches_only_the_expected_vid_and_pid() {
        assert!(is_reader_usb_device(0x1A86, 0x7523, 0x1A86, 0x7523));
        // A different chip entirely (FTDI FT232), for contrast.
        assert!(!is_reader_usb_device(0x0403, 0x6001, 0x1A86, 0x7523));
        // Same VID, different PID -- a different device from the same
        // vendor shouldn't pass either.
        assert!(!is_reader_usb_device(0x1A86, 0x0000, 0x1A86, 0x7523));
    }
}
