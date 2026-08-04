# Spec: RFID Cart Reader Desktop App

## Objective

A Windows desktop app that pairs with the ESP32+PN532 RFID cart reader (firmware in `sketch_jul20a/`) to turn a shelf of physical RFID cartridges into launchers for native PC games/apps. The user scans one or more folders of installed games, binds each cartridge to exactly one detected game, and afterward just taps a cartridge on the reader to launch it — no menus, no mouse.

**User:** solo/personal use (the app's owner), not a shared or multi-user product.

**Success looks like:** insert a bound cartridge → the right game launches. Insert an unbound one → prompted to bind it, then it auto-launches from then on. Reader missing/wrong firmware → app detects that continuously and can flash the correct firmware in-app.

Full confirmed scope, assumptions, and non-goals live in [`docs/ideas/cart-reader-desktop-app.md`](../ideas/cart-reader-desktop-app.md) — this spec operationalizes that, it doesn't re-derive it.

## Tech Stack

- **Tauri 2.x** — Rust backend, vanilla TypeScript + HTML/CSS frontend (no React/Vue/Svelte — a gallery and a handful of dialogs don't need a component framework).
- **Serial I/O**: `serialport` crate (Rust) for the reader connection.
- **Firmware flashing**: `espflash` (Rust ESP32 flashing tool/library).
- **Persistence**: single local JSON file, atomic writes (write to temp file, rename over the original) — no database.
- **Icon extraction**: SteamGridDB lookup first when `STEAMGRIDDB_API_KEY` is set (via `.env`, gitignored, loaded with `dotenvy`) and the network is reachable; falls back to local folder art, then the `windows-icons` crate (0.3.0) extracting the exe's own icon, on any failure (offline, no key, no match) — see Open Questions for how the local-fallback chain was resolved. `reqwest` (blocking) for the HTTP calls, `image` to decode/re-encode the downloaded artwork to PNG.
- **Game launching**: prefers a launcher protocol (`steam://rungameid/<appid>`, detected by reading the Steam-written `appmanifest_*.acf` next to the install folder) over spawning the exe directly, since many launcher-installed games are DRM-wrapped stubs that exit silently outside their launcher. Falls back to a direct exe spawn (with a post-launch alive-check) for anything not installed through a detected launcher. `open` crate to invoke the OS's protocol handler.
- Windows-only build target for MVP.

## Commands

To be filled in once `cargo tauri init` / `npm create tauri-app` scaffolding exists — placeholder until Plan phase, since no project has been scaffolded yet:

```
Dev:    npm run tauri dev
Build:  npm run tauri build
Test (Rust):  cargo test --manifest-path src-tauri/Cargo.toml
Lint (Rust):  cargo clippy --manifest-path src-tauri/Cargo.toml
```

## Project Structure

```
src/                    → Frontend (vanilla TS/HTML/CSS)
src/gallery/            → Gallery view, game cards
src/binding/            → Tag-bind flow, confirm-exe dialog
src/settings/           → Root folders, confirm-before-launch toggle
src-tauri/               → Rust backend
src-tauri/src/serial/    → Reader connection watchdog, protocol parsing (ID?/RFIDCART_FW:v1/INSERTED/REMOVED)
src-tauri/src/scan/      → Folder scanning, exe-detection heuristic, icon/artwork extraction
src-tauri/src/catalog/   → JSON catalog read/write, atomic persistence, availability rescan
src-tauri/src/flash/     → espflash integration for in-app firmware flashing
sketch_jul20a/           → Existing ESP32 firmware (shared protocol contract — changes here need the app side updated in lockstep)
tests/                   → Existing standalone firmware-logic checks (e.g. test_uid_to_str.cpp)
docs/ideas/, docs/spec/  → Confirmed intent and this spec
```

## Data Model

Single JSON file (path: Tauri app-data dir, e.g. `%APPDATA%/cart-reader/catalog.json`):

```json
{
  "version": 1,
  "settings": {
    "rootFolders": ["C:\\Games"],
    "confirmBeforeLaunch": false
  },
  "games": [
    {
      "id": "b1e7c9f0-3a2e-4b6a-9b1e-...",
      "name": "Some Game",
      "folderPath": "C:\\Games\\SomeGame",
      "exePath": "C:\\Games\\SomeGame\\Game.exe",
      "artworkPath": "%APPDATA%/cart-reader/artwork/b1e7c9f0.png",
      "available": true
    }
  ],
  "bindings": [
    { "tagUid": "04A3B2C1", "gameId": "b1e7c9f0-3a2e-4b6a-9b1e-..." }
  ]
}
```

Deliberately no `readerId` field on bindings — multi-reader is a non-goal, and adding it speculatively would be unused structure carried forever on the chance it's needed (see [[ideas one-pager]] Not Doing list).

## Firmware Protocol (shared contract with `sketch_jul20a.ino`)

Line-based, 115200 baud:
- `READY` — reader initialized (boot or post-removal reinit).
- `INSERTED:<UID_HEX>` / `REMOVED:<UID_HEX>` — tag events.
- `ERROR: PN532 not found - check wiring and SPI switch mode` — unrecoverable reader failure.
- **New**: host sends `ID?\n` after opening the port; firmware replies `RFIDCART_FW:v1\n`. Firmware also emits this line unprompted right after a successful `initReader()` call (boot and reinit), so a listener already attached doesn't have to ask. No reply within a short timeout after `ID?` → treated as "connected, not our firmware."

Changing this protocol requires updating both `sketch_jul20a.ino` and the app's serial parser in the same change — they are not independently versioned for MVP.

## Code Style

Rust: standard `rustfmt` defaults, `clippy`-clean. Example shape for the connection watchdog state:

```rust
enum ReaderState {
    Disconnected,
    ConnectedUnknownFirmware,
    ConnectedReady,
}
```

Frontend: TypeScript, no `any`, DOM updates via small explicit render functions rather than a reactive framework — e.g. `renderGallery(games: Game[]): void`.

## Testing Strategy

- **Rust unit tests (`cargo test`)**: firmware protocol line parser, exe-detection heuristic scoring, JSON catalog read/write round-trip (including the atomic-write path), availability-rescan logic (never mutates bindings).
- **Frontend**: no dedicated test framework for MVP — manual verification by running the app (`npm run tauri dev`) against the golden path and the edge cases below.
- **Hardware-dependent behavior** (actual serial handshake, actual flashing, actual USB unplug/replug) can only be verified against real hardware — call this out explicitly when a task can't be unit-tested for that reason, don't fake it with mocks that hide the untested part.

## Boundaries

- **Always:**
  - Run `cargo test` + `cargo clippy` before considering a Rust task done.
  - Validate user-selected folder paths exist and are readable before scanning.
  - Write the JSON catalog atomically (temp file + rename); never leave it partially written.
  - Scan staged content for secrets/credentials/malicious code before any `git commit`/`git push`.
  - At the end of each implementation phase, run both `/agent-skills:code-review-and-quality` and `/mattpocock-skills:code-review`.
  - Run `/humanizer` over any user-facing copy (dialog text, error messages, alerts) before treating it as final.
  - End the overall plan with an explicit manual checkpoint — pause for user review before calling the plan complete.
- **Ask first:**
  - Adding any Rust crate or npm dependency beyond what's named in this spec.
  - Changing the JSON schema shape once real catalog data exists.
  - Any change to the firmware serial protocol (shared contract with the `.ino`).
- **Never:**
  - Make network calls other than the opt-in SteamGridDB icon lookup (only fires when `STEAMGRIDDB_API_KEY` is set) — that lookup must fail silently and fall back to local artwork on any error, never block or crash the confirm-games flow when offline.
  - Store the SteamGridDB API key (or any credential) in `catalog.json`, source code, or anywhere else that isn't the gitignored `.env` file.
  - Silently delete or mutate catalog/binding entries — availability is tracked, entries are never auto-removed.
  - Commit the user's actual game library paths, personal artwork, or `catalog.json` contents into the repo — that's machine-specific runtime data, not project source.

## Success Criteria

- App shows connected/disconnected and firmware-identity state continuously, updating within a couple seconds of a real USB plug/unplug — not just once at launch.
- A board with missing/wrong firmware is offered an in-app flash; a board already running `RFIDCART_FW:v1` is not.
- Scanning a root folder with N subfolders produces N candidate game entries; any folder where no plausible exe is found is flagged, not silently dropped.
- Binding a tag to a game always requires an explicit user confirm of the detected exe — no path from scan to bound-and-launchable without that confirm step.
- `catalog.json` survives an app restart with all games and bindings intact, written atomically.
- Deleting a game's folder from disk, then relaunching the app, marks it unavailable without deleting its catalog/binding entry.
- Tag-insert behavior matches all four cases: bound+available → launch; bound+unavailable → alert, no launch; unbound-but-valid → bind prompt; malformed read → ignored, no crash.

## Open Questions

1. ~~Does the ESP32 board auto-reset into bootloader mode via DTR/RTS...~~ **Resolved:** yes — Arduino IDE uploads work without touching BOOT/EN, so the board has auto-reset circuitry. `flash/` can do one-click flashing via `espflash`, no manual-step UI needed.
2. ~~What's the actual best-supported way to extract a `.exe`'s embedded icon...~~ **Resolved:** the `windows-icons` crate (0.3.0) — `get_icon_by_path(path) -> Result<RgbaImage, Box<dyn Error>>`, Windows-only (matches the MVP target), thin wrapper over the Win32 icon-extraction API. Verified against a real binary (`C:\Windows\System32\notepad.exe`) — extracts and saves as PNG correctly.
3. ~~Default tie-break order for the "best-guess main exe" heuristic...~~ **Resolved:** surveyed against a real Steam library (4 installed games) — exact/substring name-match beats file-size tiebreak, recursive search (bounded depth) finds exes nested under subfolders like `pc/`, and an installer/redist filename denylist keeps `unins000.exe`/`vc_redist*.exe` out of the running. All 4 real games resolved to the correct exe with no manual correction needed.
