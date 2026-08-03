// --- Libraries ---
// Wire.h: lets the board talk I2C (not used directly here, but Adafruit_PN532 needs it available)
#include <Wire.h>
// SPI.h: lets the board talk SPI - this is the actual protocol we're using to reach the PN532
#include <SPI.h>
// Adafruit_PN532.h: the pre-written driver that knows how to talk to this specific chip
#include <Adafruit_PN532.h>

// #define creates a named constant. PN532_SS is the "chip select" pin -
// it's how the ESP32 tells the PN532 "I'm talking to you specifically" over SPI.
#define PN532_SS 5

// --- config ---
// const = this value never changes while the program runs.
// uint16_t = "unsigned 16-bit integer" - a whole number, 0 to 65535, no negatives.
const uint16_t POLL_TIMEOUT_MS = 50;   // how long each read attempt waits (in milliseconds)
// uint8_t = "unsigned 8-bit integer" - a whole number, 0 to 255. Small counters use this to save memory.
const uint8_t MISS_THRESHOLD = 4;      // consecutive empty polls before we decide the card is gone
const uint8_t REINIT_MAX_RETRIES = 2;  // how many times to retry re-initializing the reader before giving up
// --------------

// Creates the object we'll use to talk to the reader. "nfc" is just the name we chose for it -
// think of this as "build the remote control for the PN532, wired via pin 5".
Adafruit_PN532 nfc(PN532_SS);

// String = text. This holds the UID of whatever tag is currently on the reader (empty = nothing there).
// ponytail: Arduino String heap-churns on every poll (concatenation in uidToStr below) -
// fine for now, switch currentUid/uidToStr to a fixed char[15] buffer if this ever runs multi-day unattended.
String currentUid = "";
// Counts how many polls in a row came back empty - resets to 0 the moment a tag is seen again.
uint8_t missCount = 0;

// A function we wrote ourselves. It takes the raw UID bytes the reader gives us
// and turns them into a readable hex string like "04A3B2C1".
// uint8_t *uid = "a pointer to a list of bytes" (the UID). uint8_t len = how many bytes long it is.
// The function "returns" (hands back) a String when it's done.
String uidToStr(uint8_t *uid, uint8_t len) {
  String s = "";                        // start with an empty string
  for (uint8_t i = 0; i < len; i++) {    // loop over every byte in the UID, one at a time
    if (uid[i] < 0x10) s += "0";         // pad single hex digits with a leading zero (so 0xA becomes "0A")
    s += String(uid[i], HEX);            // convert this byte to hex text and stick it onto the string
  }
  s.toUpperCase();                       // make it "A3" not "a3", just for consistency
  return s;                              // hand the finished string back to whoever called this function
}

// Brings the reader up: probes it, and if it answers, runs the required SAMConfig step.
// Returns true if the reader is ready to use, false if it didn't respond.
// Shared by setup() (first boot) and the post-removal reinit loop below - both need the
// exact same "probe, configure, announce READY" sequence, so it lives in one place.
bool initReader() {
  nfc.begin();                                     // wake up and initialize the PN532 reader
  uint32_t versiondata = nfc.getFirmwareVersion();  // ask the reader "are you there? what version are you?"
  if (!versiondata) return false;                   // didn't answer - caller decides what to do

  nfc.SAMConfig();             // required setup step the PN532 needs before it can read tags
  Serial.println("READY");     // print a line over serial so the PC side knows we made it this far
  return true;
}

// setup() runs exactly ONCE, right when the board powers on or resets.
// This is where you do one-time preparation before the main program starts.
void setup() {
  Serial.begin(115200);        // open the USB serial connection at 115200 baud (must match the PC side)
  while (!Serial) { delay(10); } // wait here until the serial connection is actually ready

  if (!initReader()) {
    // Reader didn't answer - that almost always means wiring or the SPI switch mode is wrong.
    Serial.println("ERROR: PN532 not found - check wiring and SPI switch mode");
    while (1) { delay(1000); } // loop forever doing nothing - stops the program here so you notice the error
  }
}

// loop() runs over and over and over, forever, right after setup() finishes.
// This is the "main program" - everything that needs to keep happening lives here.
void loop() {
  uint8_t uid[7];      // a small array (list) to hold up to 7 bytes of UID data
  uint8_t uidLength;   // how many of those bytes actually got filled in this time

  // Ask the reader "is a tag present right now?" and wait up to POLL_TIMEOUT_MS for an answer.
  // success is true if a tag was found, false if not.
  // &uid and &uidLength pass the ADDRESS of those variables, so the function can fill them in directly.
  bool success = nfc.readPassiveTargetID(PN532_MIFARE_ISO14443A, uid, &uidLength, POLL_TIMEOUT_MS);

  if (success) {
    missCount = 0;                          // a tag answered, so reset our "how long has it been gone" counter
    String uidStr = uidToStr(uid, uidLength); // convert the raw bytes to readable text

    if (uidStr != currentUid) {
      // this is a DIFFERENT tag than what we already knew about - i.e. a new insert
      currentUid = uidStr;
      Serial.println("INSERTED:" + currentUid); // tell the PC "a new cartridge went in"
    }
    // if uidStr == currentUid, it's the same tag still sitting there - do nothing, already reported it

  } else {
    missCount++;   // no tag seen this time - add one to our "misses in a row" count
  }

  // Only declare "removed" once we've missed several polls in a row (avoids false alarms
  // from a tag that's just briefly misaligned) AND we actually have a tag on record.
  if (currentUid != "" && missCount >= MISS_THRESHOLD) {
    Serial.println("REMOVED:" + currentUid); // tell the PC "the cartridge came out"
    currentUid = "";                        // forget the tag - back to "nothing inserted"
    missCount = 0;                          // reset the counter for next time

    // Some PN532 clone boards get stuck internally after finishing a read cycle
    // and stop responding to new tags until re-initialized. Doing that here,
    // right after every removal, makes the firmware self-heal instead of
    // needing a manual reset each time. Unlike setup(), a failed reinit here
    // can't just halt on the first try - retry a few times before giving up,
    // since a mid-session halt strands whoever's using the device.
    for (uint8_t i = 0; i < REINIT_MAX_RETRIES; i++) {
      if (initReader()) break; // reader answered - stop retrying

      if (i == REINIT_MAX_RETRIES - 1) {
        // that was the last attempt and it still failed - give up like setup() does
        Serial.println("ERROR: PN532 not found - check wiring and SPI switch mode");
        while (1) { delay(1000); }
      } else {
        Serial.println("ERROR: PN532 not found, retrying...");
      }
    }
  }
}
