// Parses lines from the firmware's serial protocol (see CLAUDE.md and
// docs/spec/cart-reader-desktop-app.md for the contract), and watches the
// port for connect/disconnect + firmware-identity state.

use std::io::{BufRead, BufReader, Write};
use std::thread;
use std::time::{Duration, Instant};

pub const EXPECTED_FIRMWARE_ID: &str = "v1";
const PROBE_TIMEOUT: Duration = Duration::from_millis(800);
const POLL_INTERVAL: Duration = Duration::from_millis(700);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderState {
    Disconnected,
    ConnectedUnknownFirmware,
    ConnectedReady,
}

impl ReaderState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReaderState::Disconnected => "disconnected",
            ReaderState::ConnectedUnknownFirmware => "connectedUnknownFirmware",
            ReaderState::ConnectedReady => "connectedReady",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolEvent {
    Ready,
    Inserted(String),
    Removed(String),
    FirmwareId(String),
    Error(String),
}

/// Parses one line of firmware output. Returns `None` for anything that
/// isn't a recognized protocol line — malformed or unexpected input is
/// ignored by the caller, not treated as a crash-worthy error.
pub fn parse_line(line: &str) -> Option<ProtocolEvent> {
    let line = line.trim();

    if line == "READY" {
        return Some(ProtocolEvent::Ready);
    }
    if let Some(uid) = line.strip_prefix("INSERTED:") {
        return Some(ProtocolEvent::Inserted(uid.to_string()));
    }
    if let Some(uid) = line.strip_prefix("REMOVED:") {
        return Some(ProtocolEvent::Removed(uid.to_string()));
    }
    if let Some(id) = line.strip_prefix("RFIDCART_FW:") {
        return Some(ProtocolEvent::FirmwareId(id.to_string()));
    }
    if let Some(msg) = line.strip_prefix("ERROR:") {
        return Some(ProtocolEvent::Error(msg.trim().to_string()));
    }

    None
}

/// Finds the first USB-serial port on the system. Personal, single-board
/// use, so there's no VID/PID allowlist to maintain — first USB serial
/// port found is assumed to be the reader.
pub fn find_reader_port() -> Option<String> {
    serialport::available_ports()
        .ok()?
        .into_iter()
        .find(|p| matches!(p.port_type, serialport::SerialPortType::UsbPort(_)))
        .map(|p| p.port_name)
}

/// Opens `port_name`, sends the `ID?` handshake, and classifies the reader
/// from what comes back within `timeout`. A port that opens but never
/// answers (old/wrong firmware) is ConnectedUnknownFirmware, not an error.
pub fn probe_reader(port_name: &str, timeout: Duration) -> ReaderState {
    let port = match serialport::new(port_name, 115_200).timeout(timeout).open() {
        Ok(p) => p,
        Err(_) => return ReaderState::Disconnected,
    };

    let mut reader = BufReader::new(port);
    if reader.get_mut().write_all(b"ID?\n").is_err() {
        return ReaderState::Disconnected;
    }

    let deadline = Instant::now() + timeout;
    let mut line = String::new();
    while Instant::now() < deadline {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => match parse_line(&line) {
                Some(ProtocolEvent::FirmwareId(id)) => {
                    return if id == EXPECTED_FIRMWARE_ID {
                        ReaderState::ConnectedReady
                    } else {
                        ReaderState::ConnectedUnknownFirmware
                    };
                }
                _ => continue, // e.g. READY line before the handshake reply — keep waiting
            },
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => break,
            Err(_) => break,
        }
    }

    ReaderState::ConnectedUnknownFirmware
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchdogEvent {
    State(ReaderState),
    Tag(ProtocolEvent),
}

/// Blocks forever, polling for the reader port and its firmware identity,
/// and — once connected with the right firmware — streaming tag
/// insert/remove/error lines. Caller runs this on its own thread. Real USB
/// plug/unplug timing can only be verified against actual hardware, not in
/// a unit test.
pub fn run_watchdog<F: FnMut(WatchdogEvent)>(mut on_event: F) -> ! {
    let mut state = ReaderState::Disconnected;
    on_event(WatchdogEvent::State(state));
    loop {
        let Some(port_name) = find_reader_port() else {
            if state != ReaderState::Disconnected {
                state = ReaderState::Disconnected;
                on_event(WatchdogEvent::State(state));
            }
            thread::sleep(POLL_INTERVAL);
            continue;
        };

        let observed = probe_reader(&port_name, PROBE_TIMEOUT);
        if observed != state {
            state = observed;
            on_event(WatchdogEvent::State(state));
        }

        if state == ReaderState::ConnectedReady {
            // Blocks here streaming tag events until the connection drops
            // (unplug, or an I/O error), then falls back to polling.
            stream_tag_events(&port_name, &mut on_event);
            state = ReaderState::Disconnected;
            on_event(WatchdogEvent::State(state));
        }

        // Always pace retries — without this, a port that's enumerated but
        // fails to open every time (e.g. held exclusively by another
        // process) would spin this loop with no delay.
        thread::sleep(POLL_INTERVAL);
    }
}

/// Opens its own connection (separate from `probe_reader`'s) and reads
/// lines until the port errors out, forwarding tag insert/remove/error
/// events as they arrive.
fn stream_tag_events<F: FnMut(WatchdogEvent)>(port_name: &str, on_event: &mut F) {
    let Ok(port) = serialport::new(port_name, 115_200)
        .timeout(Duration::from_millis(1000))
        .open()
    else {
        return;
    };

    let mut reader = BufReader::new(port);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => return, // port closed
            Ok(_) => {
                if let Some(event @ (ProtocolEvent::Inserted(_) | ProtocolEvent::Removed(_) | ProtocolEvent::Error(_))) =
                    parse_line(&line)
                {
                    on_event(WatchdogEvent::Tag(event));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(_) => return, // treat any other I/O error as a disconnect
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ready() {
        assert_eq!(parse_line("READY"), Some(ProtocolEvent::Ready));
    }

    #[test]
    fn parses_inserted_with_uid() {
        assert_eq!(
            parse_line("INSERTED:04A3B2C1"),
            Some(ProtocolEvent::Inserted("04A3B2C1".to_string()))
        );
    }

    #[test]
    fn parses_removed_with_uid() {
        assert_eq!(
            parse_line("REMOVED:04A3B2C1"),
            Some(ProtocolEvent::Removed("04A3B2C1".to_string()))
        );
    }

    #[test]
    fn parses_firmware_id() {
        assert_eq!(
            parse_line("RFIDCART_FW:v1"),
            Some(ProtocolEvent::FirmwareId("v1".to_string()))
        );
    }

    #[test]
    fn parses_error_message() {
        assert_eq!(
            parse_line("ERROR: PN532 not found - check wiring and SPI switch mode"),
            Some(ProtocolEvent::Error(
                "PN532 not found - check wiring and SPI switch mode".to_string()
            ))
        );
    }

    #[test]
    fn trims_trailing_line_endings() {
        assert_eq!(parse_line("READY\r\n"), Some(ProtocolEvent::Ready));
    }

    #[test]
    fn malformed_line_is_ignored_not_crashed_on() {
        assert_eq!(parse_line("garbage noise on the wire"), None);
        assert_eq!(parse_line(""), None);
    }
}
