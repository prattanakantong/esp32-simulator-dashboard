<!-- Summary compiled by the project author. Verify against the Espressif datasheet before relying on exact numbers. -->

# ESP32 microcontroller

## ESP32 overview

The ESP32 is a microcontroller from Espressif with built-in Wi-Fi and Bluetooth. The original ESP32 has a dual-core Xtensa LX6 processor running at up to 240 MHz and 520 KB of SRAM. Wi-Fi is 802.11 b/g/n on the 2.4 GHz band; Bluetooth covers classic and Bluetooth Low Energy. Variants such as ESP32-S3 and ESP32-C3 differ in details, so check which variant a board uses.

## ESP32 logic level and power

ESP32 GPIO pins use 3.3 V logic and are not 5 V tolerant. Connecting a 5 V signal directly to a pin can damage it. Sensors powered at 3.3 V, such as a DHT22 running from the 3.3 V pin, are safe to connect directly.

## ESP32 ADC and Wi-Fi limitation

The ESP32 has a 12-bit ADC. The ADC2 channels cannot be used while Wi-Fi is active, so analog sensors in a Wi-Fi project should use ADC1 pins (GPIO 32 to 39).

## ESP32 GPIO pin notes

GPIO 34 to 39 are input only. GPIO 6 to 11 are connected to the on-board flash and must not be used. Strapping pins (such as GPIO 0, 2, 12 and 15) influence boot behavior, so avoid them for devices that could hold them at the wrong level during startup. GPIO 4 is a safe general-purpose pin and is used for the DHT22 in this project's reference firmware.

## ESP32 power consumption

Current draw peaks at a few hundred milliamps while the Wi-Fi radio transmits, so the power supply must handle short bursts. Deep sleep on the bare chip is on the order of 10 microamps, but development boards draw much more because of the USB chip and voltage regulator. For battery-powered sensors, sleep between readings and wake on a timer.

## ESP32 security features

The ESP32 supports secure boot and flash encryption, and has hardware accelerators for AES, SHA, RSA and random number generation. These features protect firmware and keys stored on the device. Enabling them in release mode burns eFuses, which is largely irreversible, so test on a spare board first. The ESP32 has no dedicated secure element; an external chip such as the ATECC608 can be added for hardware-protected private keys.

## ESP32 TLS client certificates in Arduino

With the Arduino framework, the WiFiClientSecure class supports mutual TLS using setCACert for the certificate authority, setCertificate for the device certificate and setPrivateKey for the device key. Combined with PubSubClient this lets an ESP32 connect to a broker that requires client certificates, which is how devices authenticate in this project. Each device should have its own certificate and key.

## ESP32 source and reliability

This summary is compiled by the project author from Espressif documentation and common usage. Values are typical. For design decisions, consult the official datasheet and technical reference manual.
