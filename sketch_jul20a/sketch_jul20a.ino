// PN532 RFID cartridge-presence detector for ESP32.
// Polls the reader over SPI and reports insert/remove events as plain-text
// lines over USB serial. See CLAUDE.md for the full serial protocol.

#include <Wire.h>
#include <SPI.h>
#include <Adafruit_PN532.h>

#define PN532_SS 5

const uint16_t POLL_TIMEOUT_MS = 50;
const uint8_t MISS_THRESHOLD = 4;
const uint8_t REINIT_MAX_RETRIES = 2;

// A tag held too close to the antenna over-couples it, causing reads to
// intermittently fail even though the tag never moved -- MISS_THRESHOLD
// then declares it removed, and the very next successful read re-declares
// the *same* tag inserted. This grace period treats a same-tag reappearance
// shortly after a removal as that noise, not a real re-insert.
const uint16_t REINSERT_GRACE_MS = 1000;

// Identifies this firmware to the host app, which needs to tell "real reader,
// wrong/no firmware" apart from "nothing plugged in" before it can trust any
// other line on the wire.
const char *FIRMWARE_ID = "RFIDCART_FW:v1";

Adafruit_PN532 nfc(PN532_SS);

// ponytail: Arduino String heap-churns every poll, switch to a fixed char[15] buffer if this runs multi-day unattended.
String currentUid = "";
uint8_t missCount = 0;
String lastRemovedUid = "";
unsigned long lastRemovedAtMs = 0;

// Converts raw UID bytes into an uppercase hex string, e.g. {0x04, 0xA3} -> "04A3".
String uidToStr(uint8_t *uid, uint8_t len) {
  String s = "";
  for (uint8_t i = 0; i < len; i++) {
    if (uid[i] < 0x10) s += "0";
    s += String(uid[i], HEX);
  }
  s.toUpperCase();
  return s;
}

// Probes the reader and runs the required SAMConfig step. Shared by setup()
// and the post-removal reinit loop below, since both need the same sequence.
bool initReader() {
  nfc.begin();
  uint32_t versiondata = nfc.getFirmwareVersion();
  if (!versiondata) return false;

  nfc.SAMConfig();
  Serial.println("READY");
  Serial.println(FIRMWARE_ID);
  return true;
}

// Answers the host's "ID?" handshake so it doesn't have to wait for the
// unprompted READY/FIRMWARE_ID pair after a reinit to know what it's talking to.
void handleHostCommand() {
  if (!Serial.available()) return;

  String cmd = Serial.readStringUntil('\n');
  cmd.trim();
  if (cmd == "ID?") {
    Serial.println(FIRMWARE_ID);
  }
}

// One-time boot init. Halts forever if the reader never responds, since that
// means wiring or the SPI switch mode is wrong before anything has run.
void setup() {
  Serial.begin(115200);
  while (!Serial) { delay(10); }

  if (!initReader()) {
    Serial.println("ERROR: PN532 not found - check wiring and SPI switch mode");
    while (1) { delay(1000); }
  }
}

// Polls for a tag every cycle: reports inserts immediately, and debounces
// removals over MISS_THRESHOLD misses before reporting and reinitializing
// the reader (some PN532 clones stop responding after a read cycle).
void loop() {
  handleHostCommand();

  uint8_t uid[7];
  uint8_t uidLength;

  bool success = nfc.readPassiveTargetID(PN532_MIFARE_ISO14443A, uid, &uidLength, POLL_TIMEOUT_MS);

  if (success) {
    missCount = 0;
    String uidStr = uidToStr(uid, uidLength);

    if (uidStr != currentUid) {
      bool isNoiseFromRecentRemoval = uidStr == lastRemovedUid && millis() - lastRemovedAtMs < REINSERT_GRACE_MS;
      currentUid = uidStr;
      if (!isNoiseFromRecentRemoval) {
        Serial.println("INSERTED:" + currentUid);
      }
    }
  } else {
    missCount++;
  }

  if (currentUid != "" && missCount >= MISS_THRESHOLD) {
    Serial.println("REMOVED:" + currentUid);
    lastRemovedUid = currentUid;
    lastRemovedAtMs = millis();
    currentUid = "";
    missCount = 0;

    for (uint8_t i = 0; i < REINIT_MAX_RETRIES; i++) {
      if (initReader()) break;

      if (i == REINIT_MAX_RETRIES - 1) {
        Serial.println("ERROR: PN532 not found - check wiring and SPI switch mode");
        while (1) { delay(1000); }
      } else {
        Serial.println("ERROR: PN532 not found, retrying...");
      }
    }
  }
}
