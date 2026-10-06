<!-- Describes this project's own design, so the assistant can answer questions about the system itself. -->

# Secure IoT sensor dashboard (this project)

## System architecture

Devices publish sensor readings over MQTT with mutual TLS to a Mosquitto broker. A Rust backend subscribes to the telemetry topics, stores readings in SQLite and serves a JSON API and the dashboard web page. The AI assistant is part of the same backend and can only read data through a fixed set of read-only tools.

## System MQTT topics and payload

Each device publishes to the topic devices/DEVICE_ID/telemetry, where DEVICE_ID is the device name. The payload is JSON with two numbers: temp (degrees Celsius) and hum (relative humidity percent). Timestamps are not sent by devices; the backend records the arrival time.

## System simulated devices

All devices in the demo are simulated by a Python script, not real hardware. The simulated temperature follows a slow daily sine wave around 28 degrees Celsius with a swing of about 4 degrees plus small random noise of about 0.3 degrees; humidity moves in the opposite direction. About 2 percent of readings include an injected anomaly spike of 8 to 12 degrees Celsius above normal. These spikes are deliberate, so they can be used to test whether the assistant detects abnormal values. The script publishes a reading about every 2 seconds.

## System device identity and access control

Each device and the backend have their own X.509 certificate issued by a private certificate authority. The certificate common name (CN) is the device identity. The broker accepts only clients with a valid certificate, and an access control list lets each device publish only to its own topic. A device that is compromised can therefore affect only its own data.

## System revoked devices

A device certificate can be revoked using a certificate revocation list (CRL). A revoked device is refused when it tries to connect, so it stops sending new data, but its older readings remain in the database. In the demo setup the device sim-02 was revoked, so it has history but no new readings. A revoked device that was already connected stays connected until the broker restarts.

## System interpreting reading age

The assistant reports how many seconds ago the latest reading arrived. Since devices publish about every 2 seconds, an age of more than roughly 10 to 30 seconds suggests the device or simulator is stopped, disconnected or has been revoked, and the displayed value may be stale.

## System assistant limits

The assistant is read-only. It can list devices, read the latest value, summarize a time window and search this documentation. It cannot change settings, control devices or access anything else, and it must not state sensor values that did not come from a tool result.
