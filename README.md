# Secure IoT Sensor Dashboard

A lightweight IoT telemetry pipeline where **every device is a cryptographically verified identity**. Simulated ESP32-style devices publish sensor readings over MQTT with mutual TLS, a Rust backend stores them in SQLite, and a web dashboard charts them in near real time.

The focus of this project is **machine identity and access management**: issuing, authorizing, and revoking device credentials, not just moving sensor data around.

> **Status:** Phase 1 (pipeline + dashboard) and Phase 2 (device identity) are complete and tested. Phase 3 (AI chatbot) is in progress. See [Roadmap](#roadmap).

---

## Architecture

```
┌──────────────┐  mTLS (8883)   ┌─────────────────────┐   mTLS (8883)   ┌───────────────┐
│ Simulated    │ ─────────────▶ │  Mosquitto broker   │ ──────────────▶ │ Rust backend  │
│ devices      │   publish      │  - client cert auth │   subscribe     │ (ingestor +   │
│ sim-01,      │                │  - topic ACL        │                 │  HTTP API)    │
│ sim-02 ...   │                │  - CRL revocation   │                 └───────┬───────┘
└──────────────┘                └─────────────────────┘                         │
                                                                          ┌─────▼─────┐
                                                           HTTP API       │  SQLite   │
                                              Dashboard ◀──────────────── └───────────┘
                                              (HTML + Chart.js)
```

| Component        | Technology                                    | Role                                                           |
| ---------------- | --------------------------------------------- | -------------------------------------------------------------- |
| Device simulator | Python, `paho-mqtt`                           | Publishes temperature/humidity, occasionally injects anomalies |
| Broker           | Eclipse Mosquitto 2.x                         | TLS termination, authentication, authorization, revocation     |
| Backend          | Rust (`axum`, `tokio`, `rumqttc`, `rusqlite`) | MQTT subscriber, SQLite writer, JSON API                       |
| Storage          | SQLite                                        | Time-series readings (indexed by device and timestamp)         |
| Dashboard        | HTML + Chart.js                               | Per-device charts                                              |

### Design choices

- **Lightweight by design.** Native Mosquitto, a single Rust binary, and SQLite keep the whole stack to a few tens of MB of RAM. No Docker required.
- **Simulation first.** Devices are simulated in Python. The topic and payload format is a contract (below), so real ESP32 firmware can replace the simulator without touching any other component.
- **SQLite instead of a time-series DB.** Appropriate for this scale. The storage code is isolated so it can be swapped for TimescaleDB.

### Message contract

| Item      | Value                                                         |
| --------- | ------------------------------------------------------------- |
| Topic     | `devices/<device_id>/telemetry`                               |
| Payload   | `{"temp": <float>, "hum": <float>}`                           |
| Timestamp | Assigned by the backend on receipt (devices do not send time) |

---

## Identity & access model

This section is the core of the project. The concepts map directly onto classic IAM.

| IAM concept    | Implementation                                                                              |
| -------------- | ------------------------------------------------------------------------------------------- |
| Principal      | Each device (`sim-01`, `sim-02`) and the backend (`ingestor`)                               |
| Credential     | X.509 client certificate + private key, one per principal                                   |
| Identity       | Certificate **CN**, exposed to the broker as the MQTT username (`use_identity_as_username`) |
| Trust anchor   | Private CA (`IoT-Dev-CA`)                                                                   |
| Authentication | Mutual TLS: the broker rejects any client without a certificate signed by the CA            |
| Authorization  | Mosquitto ACL bound to the CN (least privilege)                                             |
| Provisioning   | Issue a key + CSR, sign it with the CA                                                      |
| Deprovisioning | Revoke the certificate and publish a CRL                                                    |

### Authorization rules (`mosquitto/acl.conf`)

```
user ingestor
topic read devices/+/telemetry

pattern write devices/%u/telemetry
```

- Devices can **only publish** to `devices/<their own CN>/telemetry`.
- Devices have **no read access** at all.
- The `ingestor` can **only read** telemetry topics.
- `%u` resolves to the CN, so adding a device requires no ACL change.
- Anything not explicitly allowed is denied.

### Threat model

| Threat                                                  | Mitigation                                                                                          |
| ------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| Unknown client connects to the broker                   | No plaintext listener; `require_certificate true` rejects clients without a CA-signed cert          |
| A device impersonates another device                    | Identity is bound to a certificate; forging a CN requires the CA key or the victim's private key    |
| A compromised device writes into another device's topic | ACL `pattern write` limits each CN to its own topic. Blast radius is one topic                      |
| A compromised device snoops on other devices            | Devices have no read permission                                                                     |
| Decommissioned or stolen device keeps connecting        | Certificate revoked via CRL; new connections are refused                                            |
| Malformed or hostile payloads                           | Backend parses into a typed struct (`serde`); invalid messages are logged and dropped, never stored |
| Eavesdropping / tampering in transit                    | TLS 1.2 on the only listener (8883)                                                                 |

### Verified by test

These were run against the live broker:

1. **No client certificate** → connection rejected.
2. **`sim-01` publishes to its own topic** → accepted and stored.
3. **`sim-01` publishes to `devices/sim-02/telemetry`** → denied by the ACL; the broker logs the denial and the message never reaches the subscriber.
4. **`sim-02` certificate revoked** → `openssl verify -crl_check` reports `certificate revoked`, and new connections using it are refused, while `sim-01` and the `ingestor` continue to work unaffected.

### Known limitations

Stated openly, because they define the boundary of this design:

- **CRL does not terminate existing sessions.** A revoked device that is already connected stays connected until it disconnects. Restarting the broker (or using Mosquitto's dynamic security plugin) is required to force it off.
- **CRL must be regenerated** before its validity window (30 days) expires, otherwise validation can fail.
- **Private keys are plain files.** There is no secure element or hardware-backed key storage, so on a real device a key could be extracted. Production hardware should use a secure element (e.g. ATECC608) or the ESP32's flash encryption and secure boot.
- **Certificate lifetime is 365 days with no automated renewal.** Rotation is manual.
- **The CA key lives on the same machine.** In production it would be kept offline or in an HSM.
- **`pattern write` applies to every user**, so the `ingestor` technically gains write access to `devices/ingestor/telemetry`. Harmless here, but a stricter design would split device and service topic namespaces.
- **Devices do not send timestamps**, so ordering reflects arrival time, not measurement time.
- **Development-grade certificates:** the broker certificate is issued for `localhost` only.

---

## Project structure

```
.
├── backend/              Rust service (MQTT ingestor + HTTP API)
│   ├── Cargo.toml
│   └── src/main.rs
├── frontend/
│   └── index.html        Dashboard (embedded into the backend binary at build time)
├── simulator/
│   └── esp32.py          Device simulator
├── mosquitto/
│   ├── mosquitto.conf    Broker config (use mosquitto.conf.example, see below)
│   └── acl.conf          Topic ACL
├── certs/                Generated locally. Private keys are NOT committed
└── .gitignore
```

---

## Getting started

### Prerequisites

- [Mosquitto 2.x](https://mosquitto.org/download/)
- [Rust toolchain](https://rustup.rs) (`cargo`)
- Python 3.9+
- OpenSSL (on Windows, use Git Bash, which bundles it)

### 1. Python environment

```bash
python -m venv venv
# Windows: venv\Scripts\activate    |  Linux/macOS: source venv/bin/activate
python -m pip install "paho-mqtt>=2"
```

### 2. Generate certificates

Run in `certs/`. On Windows Git Bash, write `//CN=...` instead of `/CN=...` to avoid path conversion.

```bash
mkdir -p certs && cd certs

# Certificate authority
openssl genrsa -out ca.key 4096
openssl req -x509 -new -key ca.key -sha256 -days 3650 -subj "/CN=IoT-Dev-CA" -out ca.crt

# Broker certificate (SAN required for hostname verification)
openssl genrsa -out broker.key 2048
openssl req -new -key broker.key -subj "/CN=localhost" -out broker.csr
cat > broker.ext <<EOF
subjectAltName = DNS:localhost, IP:127.0.0.1
extendedKeyUsage = serverAuth
EOF
openssl x509 -req -in broker.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
  -out broker.crt -days 365 -sha256 -extfile broker.ext

# Client certificates (devices and backend)
issue_client() {
  NAME=$1
  openssl genrsa -out $NAME.key 2048
  openssl req -new -key $NAME.key -subj "/CN=$NAME" -out $NAME.csr
  echo "extendedKeyUsage = clientAuth" > $NAME.ext
  openssl x509 -req -in $NAME.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
    -out $NAME.crt -days 365 -sha256 -extfile $NAME.ext
}
issue_client sim-01
issue_client sim-02
issue_client ingestor
```

Check: `openssl verify -CAfile ca.crt sim-01.crt sim-02.crt ingestor.crt` should print `OK` for each.

### 3. Configure and start the broker

Create `mosquitto/mosquitto.conf`. Use **absolute paths** for the certificate files, since Mosquitto does not resolve them relative to your working directory on Windows. Use forward slashes and ASCII only (no non-ASCII characters in config files).

```
listener 8883
protocol mqtt

cafile /ABSOLUTE/PATH/certs/ca.crt
certfile /ABSOLUTE/PATH/certs/broker.crt
keyfile /ABSOLUTE/PATH/certs/broker.key

tls_version tlsv1.2
require_certificate true
use_identity_as_username true

allow_anonymous false
acl_file /ABSOLUTE/PATH/mosquitto/acl.conf
```

Start it and leave it running:

```bash
mosquitto -c mosquitto/mosquitto.conf -v
```

On Windows PowerShell use `& "C:\Program Files\mosquitto\mosquitto.exe" -c mosquitto\mosquitto.conf -v`. You should see only port **8883** opened.

### 4. Start the backend

```bash
cd backend
cargo run
```

Expect `Listening on http://127.0.0.1:8000` and `MQTT connected`. The backend reads its certificates from `../certs/`, so run it from inside `backend/`. The first build takes a few minutes.

### 5. Start simulated devices

From the project root, in a shell with the venv active:

```bash
python simulator/esp32.py sim-01
python simulator/esp32.py sim-02
```

Open <http://localhost:8000> and pick a device.

### 6. Try revoking a device

Set up a CA database in `certs/`:

```bash
touch index.txt
echo 1000 > crlnumber
cat > ca.cnf <<'EOF'
[ca]
default_ca = CA_default

[CA_default]
database = index.txt
crlnumber = crlnumber
default_md = sha256
default_crl_days = 30
EOF
```

Because the certificates above were signed with `openssl x509 -req`, they are not recorded in `index.txt`. Register the one you want to revoke (here `sim-02`) first:

```bash
SERIAL=$(openssl x509 -in sim-02.crt -noout -serial | cut -d= -f2)
ENDDATE=$(openssl x509 -in sim-02.crt -noout -enddate | cut -d= -f2)
EXPIRY=$(date -u -d "$ENDDATE" +%y%m%d%H%M%SZ)
printf "V\t%s\t\t%s\tunknown\t/CN=sim-02\n" "$EXPIRY" "$SERIAL" > index.txt
```

Revoke and generate the CRL:

```bash
openssl ca -config ca.cnf -cert ca.crt -keyfile ca.key -revoke sim-02.crt
openssl ca -config ca.cnf -cert ca.crt -keyfile ca.key -gencrl -out crl.pem
openssl verify -crl_check -CAfile ca.crt -CRLfile crl.pem sim-02.crt   # expect: certificate revoked
```

Add `crlfile /ABSOLUTE/PATH/certs/crl.pem` to `mosquitto.conf` and **restart the broker**. `sim-02` is now refused, while `sim-01` and the backend keep working.

---

## API

| Endpoint                                    | Description                                                  |
| ------------------------------------------- | ------------------------------------------------------------ |
| `GET /api/devices`                          | List of device IDs that have reported data                   |
| `GET /api/readings?device=<id>&minutes=<n>` | Readings for a device over the last `n` minutes (default 60) |

Note: `/api/devices` reads from stored history, so a revoked device still appears (its past data remains) but stops receiving new readings.

---

## Security notes for contributors

- **Never commit private keys.** `.gitignore` excludes `certs/*.key`, the serial files, and the CA database files.
- The repository does not include working certificates. Generate your own with the steps above.
- Treat device-originated data as untrusted input, especially once it is passed to any language model.

---

## Roadmap

- [x] **Phase 1:** Simulated sensors → MQTT → Rust ingestor → SQLite → dashboard
- [x] **Phase 2:** mTLS authentication, per-device ACL authorization, CRL revocation
- [ ] **Phase 3:** AI assistant
  - [ ] `/api/chat` endpoint backed by an LLM API (kept behind a single module so the provider can be swapped)
  - [ ] Read-only tools for latest and historical sensor values
  - [ ] Lightweight RAG over sensor datasheets (SQLite FTS5)
  - [ ] Chat panel in the dashboard
  - [ ] Prompt-injection hardening: device data is passed as quoted data, never as instructions; the assistant has no write or actuator access
- [ ] Scripted provisioning (`scripts/gen-certs.sh`) and automated CRL refresh
- [ ] Real ESP32 firmware using the same topic and payload contract

---

## License

Choose a license (for example MIT) and add a `LICENSE` file.
