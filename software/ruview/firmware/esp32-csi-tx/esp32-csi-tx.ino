#include <WiFi.h>
#include <WiFiUdp.h>

#include "tx_config.h"
#include "tx_packet.h"

namespace {

constexpr uint8_t kWifiChannel = 6;
constexpr uint16_t kSoundingPort = 4210;
constexpr uint32_t kSoundingIntervalMs = 20;
constexpr uint8_t kIdentityRed = 255;
constexpr uint8_t kIdentityGreen = 159;
constexpr uint8_t kIdentityBlue = 10;
constexpr uint8_t kLedBrightness = 32;

#ifndef RGB_BUILTIN
constexpr uint8_t RGB_BUILTIN = 48;
#endif

WiFiUDP udp;
uint32_t sequence = 0;
uint32_t nextSendAtMs = 0;

uint8_t scaleLedChannel(uint8_t channel) {
  return static_cast<uint8_t>((static_cast<uint16_t>(channel) * kLedBrightness + 127U) / 255U);
}

void showIdentityColor() {
  rgbLedWrite(
    RGB_BUILTIN,
    scaleLedChannel(kIdentityRed),
    scaleLedChannel(kIdentityGreen),
    scaleLedChannel(kIdentityBlue));
}

void sendSoundingPacket() {
  uint8_t packet[csi_tx::kSoundingPacketSize];
  const uint32_t nowMs = millis();

  csi_tx::buildSoundingPacket(packet, sequence, nowMs);

  udp.beginPacket(IPAddress(192, 168, 4, 255), kSoundingPort);
  udp.write(packet, sizeof(packet));
  udp.endPacket();
  ++sequence;
}

}  // namespace

void setup() {
  Serial.begin(115200);
  delay(3000);

  showIdentityColor();
  Serial.printf("Boot OK: %s, identity color #%02x%02x%02x\n",
                csi_tx::kNodeId, kIdentityRed, kIdentityGreen, kIdentityBlue);
  Serial.println("Starting WiFi in 3 seconds...");
  delay(3000);

  WiFi.mode(WIFI_AP);
  delay(500);
  WiFi.setTxPower(WIFI_POWER_11dBm);
  WiFi.softAPsetHostname(csi_tx::kNodeId);
  delay(500);

  const bool started = WiFi.softAP(
    CSI_TX_WIFI_SSID,
    CSI_TX_WIFI_PASSWORD,
    kWifiChannel,
    false,
    6);
  if (!started) {
    Serial.println("SoftAP failed");
    return;
  }

  Serial.printf("SoftAP started: identity=%s channel=%u\n", csi_tx::kNodeId, kWifiChannel);
  Serial.print("AP IP: ");
  Serial.println(WiFi.softAPIP());

  udp.begin(kSoundingPort);
  nextSendAtMs = millis();
}

void loop() {
  const uint32_t nowMs = millis();
  if (static_cast<int32_t>(nowMs - nextSendAtMs) < 0) {
    delay(1);
    return;
  }

  sendSoundingPacket();
  nextSendAtMs += kSoundingIntervalMs;
  if (static_cast<int32_t>(nowMs - nextSendAtMs) >= 0) {
    nextSendAtMs = nowMs + kSoundingIntervalMs;
  }
}
