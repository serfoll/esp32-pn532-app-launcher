// PN532 RFID cartridge-presence detector for ESP32.
// Polls the reader over SPI and reports insert/remove events as plain-text
// lines over USB serial. See CLAUDE.md for the full serial protocol.

#include <Wire.h>          // required by Adafruit_PN532 even though we talk SPI
#include <SPI.h>
#include <Adafruit_PN532.h>

#define PN532_SS 5  // chip-select pin for the PN532 over SPI

const uint16_t POLL_TIMEOUT_MS = 50;   // how long each read attempt waits, in ms
const uint8_t MISS_THRESHOLD = 4;      // consecutive empty polls before declaring "removed"
const uint8_t REINIT_MAX_RETRIES = 2;  // reinit attempts after a removal before giving up

Adafruit_PN532 nfc(PN532_SS);

// ponytail: Arduino String heap-churns on every poll (concatenation in uidToStr below) -
// fine for now, switch currentUid/uidToStr to a fixed char[15] buffer if this ever runs multi-day unattended.
String currentUid = "";  // UID of the tag currently on the reader, empty if none
uint8_t missCount = 0;    // consecutive empty polls since the tag was last seen

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
  return true;
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
// removals over MISS_THRESHOLD misses before reporting and reinitializing.
void loop() {
  uint8_t uid[7];
  uint8_t uidLength;

  bool success = nfc.readPassiveTargetID(PN532_MIFARE_ISO14443A, uid, &uidLength, POLL_TIMEOUT_MS);

  if (success) {
    missCount = 0;
    String uidStr = uidToStr(uid, uidLength);

    if (uidStr != currentUid) {
      currentUid = uidStr;
      Serial.println("INSERTED:" + currentUid);
    }
  } else {
    missCount++;
  }

  if (currentUid != "" && missCount >= MISS_THRESHOLD) {
    Serial.println("REMOVED:" + currentUid);
    currentUid = "";
    missCount = 0;

    // Some PN532 clones stop responding after a read cycle finishes; reinit
    // here so the firmware self-heals. Unlike setup(), retry a few times
    // before giving up, since halting mid-session strands whoever's using it.
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
