use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{Query, State},
    response::Html,
    routing::get,
    Json, Router,
};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

type Db = Arc<Mutex<Connection>>;

#[derive(Deserialize)]
struct Telemetry {
    temp: f64,
    hum: f64,
}

#[derive(Serialize)]
struct Reading {
    ts: f64,
    temp: f64,
    hum: f64,
}

#[derive(Deserialize)]
struct ReadingsQuery {
    device: String,
    #[serde(default = "default_minutes")]
    minutes: u64,
}

fn default_minutes() -> u64 {
    60
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

fn init_db() -> Connection {
    let conn = Connection::open("iot.db").expect("เปิดไฟล์ DB ไม่ได้");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS readings(
            id INTEGER PRIMARY KEY,
            device_id TEXT NOT NULL,
            ts REAL NOT NULL,
            temp REAL,
            hum REAL);
         CREATE INDEX IF NOT EXISTS idx_dev_ts ON readings(device_id, ts);",
    )
    .expect("สร้างตารางไม่ได้");
    conn
}

// ---------- ฝั่ง MQTT: subscriber ----------
async fn mqtt_ingest(db: Db) {
    let mut opts = MqttOptions::new("ingestor", "localhost", 1883);
    opts.set_keep_alive(Duration::from_secs(10));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);

    loop {
        match eventloop.poll().await {
            // เชื่อมต่อสำเร็จ (รวมถึงตอนต่อใหม่) -> สมัครรับ topic
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                println!("MQTT connected");
                if let Err(e) = client
                    .subscribe("devices/+/telemetry", QoS::AtMostOnce)
                    .await
                {
                    eprintln!("subscribe failed: {e}");
                }
            }
            Ok(Event::Incoming(Packet::Publish(p))) => {
                // topic หน้าตา devices/<id>/telemetry
                let parts: Vec<&str> = p.topic.split('/').collect();
                if parts.len() != 3 {
                    continue;
                }
                match serde_json::from_slice::<Telemetry>(&p.payload) {
                    Ok(t) => {
                        let conn = db.lock().unwrap();
                        if let Err(e) = conn.execute(
                            "INSERT INTO readings(device_id, ts, temp, hum) VALUES (?1,?2,?3,?4)",
                            params![parts[1], now(), t.temp, t.hum],
                        ) {
                            eprintln!("db error: {e}");
                        }
                    }
                    Err(e) => eprintln!("bad message: {e}"),
                }
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("mqtt error: {e}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

// ---------- ฝั่ง HTTP API ----------
async fn devices(State(db): State<Db>) -> Json<Vec<String>> {
    let conn = db.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT DISTINCT device_id FROM readings")
        .unwrap();
    let rows: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    Json(rows)
}

async fn readings(
    State(db): State<Db>,
    Query(q): Query<ReadingsQuery>,
) -> Json<Vec<Reading>> {
    let since = now() - (q.minutes * 60) as f64;
    let conn = db.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT ts, temp, hum FROM readings WHERE device_id=?1 AND ts>?2 ORDER BY ts")
        .unwrap();
    let rows: Vec<Reading> = stmt
        .query_map(params![q.device, since], |r| {
            Ok(Reading {
                ts: r.get(0)?,
                temp: r.get(1)?,
                hum: r.get(2)?,
            })
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    Json(rows)
}

#[tokio::main]
async fn main() {
    let db: Db = Arc::new(Mutex::new(init_db()));

    tokio::spawn(mqtt_ingest(db.clone()));

    let app = Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../../frontend/index.html")) }),
        )
        .route("/api/devices", get(devices))
        .route("/api/readings", get(readings))
        .with_state(db);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8000")
        .await
        .unwrap();
    println!("Listening on http://127.0.0.1:8000");
    axum::serve(listener, app).await.unwrap();
}