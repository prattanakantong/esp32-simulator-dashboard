import json
import math
import random
import sys
import time

import paho.mqtt.client as mqtt

DEVICE_ID = sys.argv[1] if len(sys.argv) > 1 else "sim-01"
MQTT_HOST = "localhost"
MQTT_PORT = 8883
INTERVAL = 2
TOPIC = f"devices/{DEVICE_ID}/telemetry"
CERT_DIR = "certs"

client = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2, client_id=DEVICE_ID)
client.tls_set(
    ca_certs=f"{CERT_DIR}/ca.crt",
    certfile=f"{CERT_DIR}/{DEVICE_ID}.crt",
    keyfile=f"{CERT_DIR}/{DEVICE_ID}.key",
)
client.connect(MQTT_HOST, MQTT_PORT)
client.loop_start()

temp = 28.0
start = time.time()

while True:
    # อุณหภูมิเปลี่ยนตามรอบวันแบบหยาบๆ + noise เล็กน้อย
    elapsed_h = (time.time() - start) / 3600
    base = 28 + 4 * math.sin(elapsed_h * 2 * math.pi / 24)
    temp = base + random.uniform(-0.3, 0.3)
    hum = 60 - (temp - 28) * 2 + random.uniform(-1, 1)

    # 2% ของรอบ ให้เกิดค่าผิดปกติ ไว้ใช้ demo chatbot
    if random.random() < 0.02:
        temp += random.uniform(8, 12)

    payload = {"temp": round(temp, 2), "hum": round(hum, 2)}
    client.publish(TOPIC, json.dumps(payload))
    print(TOPIC, payload)
    time.sleep(INTERVAL)