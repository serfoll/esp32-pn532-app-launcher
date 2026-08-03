// Standalone check for the hex-formatting logic in sketch_jul20a.ino's uidToStr().
// Arduino's String/SPI/Wire headers aren't available outside the Arduino toolchain, so this
// mirrors the same pad+hex+uppercase logic in plain C++ to catch a regression without hardware.
// Build & run: g++ -std=c++17 -o test_uid_to_str tests/test_uid_to_str.cpp && ./test_uid_to_str
#include <cassert>
#include <cstdint>
#include <cstdio>
#include <string>

std::string uidToStr(const uint8_t *uid, uint8_t len) {
  std::string s;
  for (uint8_t i = 0; i < len; i++) {
    if (uid[i] < 0x10) s += "0";
    char buf[3];
    std::snprintf(buf, sizeof(buf), "%X", uid[i]);
    s += buf;
  }
  return s;
}

int main() {
  uint8_t uid4[] = {0x04, 0xA3, 0xB2, 0xC1};
  assert(uidToStr(uid4, 4) == "04A3B2C1");

  uint8_t uid1[] = {0x00};
  assert(uidToStr(uid1, 1) == "00");

  uint8_t uid7[] = {0xFF, 0x01, 0x0A, 0x10, 0x00, 0xAB, 0xCD};
  assert(uidToStr(uid7, 7) == "FF010A1000ABCD");

  std::printf("uidToStr: all checks passed\n");
  return 0;
}
