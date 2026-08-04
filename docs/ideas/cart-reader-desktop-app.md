# RFID Cart Reader Desktop App

## Problem Statement

How might we let a solo PC-gamer/tinkerer physically launch a native PC game or app by tapping a labeled RFID cartridge on a reader they built themselves — with the whole stack (detection, cataloging, launching) under their own control, fully offline?

## Recommended Direction

A standalone Tauri (Rust + Node) Windows desktop app, paired one-to-one with the ESP32+PN532 cart reader whose firmware already exists. The app owns three jobs: (1) keep a continuously-monitored connection to the reader and confirm it's running the right firmware, offering an in-app flash when it isn't; (2) let the user point at folders of installed games, best-guess the main executable per folder, and have the user confirm/override that guess once when they bind a tag to it; (3) watch the serial stream for tag events and launch the bound game — instantly by default, with an optional short confirm window available in settings for anyone who wants a guard against accidental bumps.

Standalone was a deliberate call, not a gap: existing launchers (Playnite, LaunchBox) were considered and ruled out — the point of this project is owning the full loop, not gluing RFID onto someone else's tool. That also means no attempt to reuse their scanning/artwork pipelines; this app builds its own, intentionally smaller version of the same idea.

## Key Assumptions to Validate

- [ ] The specific ESP32 board auto-resets into bootloader mode via DTR/RTS toggling (needed for in-app flashing without asking the user to hold a BOOT button) — check on the actual hardware before building the flashing UI around it.
- [ ] The "best-guess main exe" heuristic (name match / largest exe / has an icon) is good enough in practice not to annoy the user on their real game library — untested against real folders so far.
- [ ] The continuous connection watchdog can reliably detect and recover from a real USB unplug/replug and Windows COM port renumbering, not just the "board never disconnects" happy path.
- [ ] A single local JSON file stays adequate as the catalog grows over a personal library (dozens–low hundreds of entries, not thousands).

## MVP Scope

**In:**
- Reader connection watchdog (continuous, not one-shot) + firmware identity handshake (`ID?` → `RFIDCART_FW:v1`) + in-app flashing when firmware is missing/wrong.
- Folder-based game scanning, one subfolder = one candidate game, best-guess exe detection with mandatory user confirm/override when binding.
- Local-only artwork: folder/cover/box image file first, `.exe` icon extraction as fallback.
- Gallery view of detected games.
- One tag → one specific game binding (not folder-level), stored in a single local JSON file.
- Background rescan on app launch to refresh availability without ever mutating existing bindings.
- Tag insert behavior: bound+available → instant launch (confirm-window togglable in settings); bound+unavailable → alert, no launch; unrecognized-but-valid tag → prompt to bind; malformed serial read → silently ignored.

**Out:**
- ROM/emulator support.
- Online artwork/metadata lookup (SteamGridDB, IGDB).
- Multiple simultaneous readers.
- Cloud sync / multi-machine.
- Per-game launch args / working directory configuration.
- Playnite/LaunchBox integration of any kind.

## Not Doing (and Why)

- **Playnite/LaunchBox plugin instead of standalone** — considered and rejected; owning the full loop is the point of the project, not a gap to fill later.
- **Fully-automatic exe detection with no confirm step** — real launchers treat this heuristic as unreliable (game folders routinely have 5+ exes: launcher, config tool, uninstaller, redist installers); shipping it fully automatic risks silently binding a tag to the wrong program with no easy fix.
- **Speculative `reader_id` field in the JSON schema "for future multi-reader support"** — multi-reader is out of scope and unconfirmed to ever happen; adding unused structure now is exactly the kind of thing to skip. Migrate the schema if that day actually comes.
- **One-time connection check at launch** — rejected in favor of a continuous watchdog, since the app is meant to sit running in the background indefinitely and USB state changes mid-session.

## Open Questions

- Does the ESP32 board in hand auto-reset into bootloader mode, or will in-app flashing need a "hold BOOT, click Flash" manual step in the UI?
- What's the actual shape of the user's game library (how many folders realistically have >1 exe) — worth a quick manual survey before finalizing the detection heuristic's ranking order.
