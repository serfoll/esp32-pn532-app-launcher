# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Firmware for an ESP32 that uses an Adafruit PN532 NFC/RFID reader (SPI mode) to detect when an RFID-tagged cartridge is inserted or removed, and reports those events over USB serial. There is currently a single sketch: `sketch_jul20a/sketch_jul20a.ino`.

This repo has no build tooling configured (no `arduino-cli` config, no `.vscode` settings, no CI). Compiling/uploading is done via whatever Arduino toolchain the user has set up locally (Arduino IDE or `arduino-cli`), targeting an ESP32 board with the `Adafruit_PN532` library (and its `Wire`/`SPI` dependencies) installed.

## Architecture

Everything lives in one file and runs as a single polling loop — there's no async/interrupt-driven logic:

- **Wiring/config**: PN532 is wired over SPI with chip-select on pin `PN532_SS` (5). Tuning constants at the top (`POLL_TIMEOUT_MS`, `MISS_THRESHOLD`, `REINIT_MAX_RETRIES`) control poll responsiveness, debounce-on-removal, and reinit retry behavior.
- **`setup()`**: initializes serial (115200 baud), brings up the PN532, and halts forever (`while(1)`) if the reader doesn't respond — this is a hard failure with no recovery, by design, since it means wiring/SPI mode is wrong before anything has run.
- **`loop()`**: polls for a tag each cycle. A UID different from the currently-tracked one is treated as a new insert; the tag is only declared "removed" after `MISS_THRESHOLD` consecutive empty polls (debounces momentary misreads, not just single dropped polls).
- **Self-healing reinit**: some PN532 clone boards stop responding after a read cycle completes. After every detected removal, the firmware reinitializes the reader (`nfc.begin()` + `SAMConfig()`), retrying up to `REINIT_MAX_RETRIES` times before giving up — unlike `setup()`'s hard halt, this exists because a mid-session halt would strand whoever is using the device.
- **Serial protocol** (the contract with whatever reads this over USB): one line per event —
  - `READY` — reader initialized successfully (on boot, and again after a successful reinit)
  - `INSERTED:<UID_HEX>` — new tag detected, UID as uppercase hex
  - `REMOVED:<UID_HEX>` — previously-tracked tag confirmed gone
  - `ERROR: PN532 not found - check wiring and SPI switch mode` — unrecoverable reader failure

## Project context

This codebase doubles as a teaching workspace: the user is an experienced JS/Python developer who is new to embedded C/Arduino, and the existing code is deliberately over-commented to explain basic concepts (fixed-width integer types, pointers/`&`, `.ino` structure, etc.) inline. When adding new code or explaining this one, keep that same explain-as-you-go density rather than assuming embedded/C familiarity, and when teaching from this file, walk through it in the order things appear (top to bottom) rather than jumping to what seems like the most important concept first.
