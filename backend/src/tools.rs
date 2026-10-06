use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use std::fs;

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// device id ที่ยอมให้ผ่านไปถึง LLM: ตัวอักษร/ตัวเลข/-/_ เท่านั้น
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// รายการ tool ที่บอก Gemini
pub fn declarations() -> Value {
    json!([{
        "functionDeclarations": [
            {
                "name": "list_devices",
                "description": "List the IDs of all devices that have reported sensor data."
            },
            {
                "name": "get_latest_reading",
                "description": "Get the most recent temperature (Celsius) and humidity (percent) reading of one device, and how many seconds ago it was received.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "device_id": { "type": "string", "description": "Device ID, e.g. sim-01" }
                    },
                    "required": ["device_id"]
                }
            },
            {
                "name": "get_summary",
                "description": "Get count, min, max and average of temperature and humidity of one device over the last N minutes.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "device_id": { "type": "string", "description": "Device ID, e.g. sim-01" },
                        "minutes": { "type": "integer", "description": "Window size in minutes, 1 to 1440" }
                    },
                    "required": ["device_id", "minutes"]
                }
            },
            {
                "name": "search_docs",
                "description": "Search the project's hardware documentation: DHT22 sensor, ESP32 board, and how this system works (architecture, simulated data, certificates, access control). Use it for any question about hardware specs, wiring, troubleshooting or system design. Write the query as a few English keywords, even if the user writes in another language.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "A few English keywords, e.g. 'DHT22 accuracy'" }
                    },
                    "required": ["query"]
                }
            } 
        ]
    }])
}

/// ตรวจ device_id ที่ LLM ส่งมา (ถือเป็น input ที่ไม่น่าเชื่อถือ)
fn device_arg(conn: &Connection, args: &Value) -> Result<String, Value> {
    let id = args["device_id"].as_str().unwrap_or("");
    if !valid_id(id) {
        return Err(json!({ "error": "invalid device_id" }));
    }
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM readings WHERE device_id = ?1)",
            params![id],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if !exists {
        return Err(json!({ "error": "unknown device_id" }));
    }
    Ok(id.to_string())
}

/// รัน tool ที่ LLM ขอ ทุกอย่างเป็น SELECT แบบ parameterized เท่านั้น
pub fn run(conn: &Connection, name: &str, args: &Value) -> Value {
    match name {
        "list_devices" => list_devices(conn),
        "get_latest_reading" => match device_arg(conn, args) {
            Ok(id) => latest(conn, &id),
            Err(e) => e,
        },
        "get_summary" => match device_arg(conn, args) {
            Ok(id) => {
                let minutes = args["minutes"]
                    .as_f64()
                    .map(|f| f as i64)
                    .unwrap_or(60)
                    .clamp(1, 1440);
                summary(conn, &id, minutes)
            }
            Err(e) => e,
        },
        "search_docs" => search_docs(conn, args),
        _ => json!({ "error": "unknown tool" }),
    }
}

fn list_devices(conn: &Connection) -> Value {
    let mut stmt = match conn.prepare("SELECT DISTINCT device_id FROM readings LIMIT 50") {
        Ok(s) => s,
        Err(_) => return json!({ "error": "database error" }),
    };
    let ids: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map(|rows| rows.filter_map(Result::ok).filter(|s| valid_id(s)).collect())
        .unwrap_or_default();
    json!({ "devices": ids })
}

fn latest(conn: &Connection, id: &str) -> Value {
    conn.query_row(
        "SELECT ts, temp, hum FROM readings WHERE device_id = ?1 ORDER BY ts DESC LIMIT 1",
        params![id],
        |r| {
            let ts: f64 = r.get(0)?;
            let temp: f64 = r.get(1)?;
            let hum: f64 = r.get(2)?;
            Ok(json!({
                "temp_c": round2(temp),
                "humidity_pct": round2(hum),
                "age_seconds": (now() - ts).max(0.0).round()
            }))
        },
    )
    .unwrap_or_else(|_| json!({ "error": "database error" }))
}

fn summary(conn: &Connection, id: &str, minutes: i64) -> Value {
    let since = now() - (minutes * 60) as f64;
    conn.query_row(
        "SELECT COUNT(*), MIN(temp), MAX(temp), AVG(temp), MIN(hum), MAX(hum), AVG(hum)
         FROM readings WHERE device_id = ?1 AND ts > ?2",
        params![id, since],
        |r| {
            let g = |i: usize| r.get::<_, Option<f64>>(i).map(|o| o.map(round2));
            let count: i64 = r.get(0)?;
            Ok(json!({
                "window_minutes": minutes,
                "count": count,
                "temp_c": { "min": g(1)?, "max": g(2)?, "avg": g(3)? },
                "humidity_pct": { "min": g(4)?, "max": g(5)?, "avg": g(6)? }
            }))
        },
    )
    .unwrap_or_else(|_| json!({ "error": "database error" }))
    
}

/// อ่านไฟล์ .md ในโฟลเดอร์ แล้วสร้างดัชนี FTS5 ใหม่ทุกครั้งที่เริ่มโปรแกรม
pub fn init_docs(conn: &Connection, dir: &str) -> usize {
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS docs_fts USING fts5(
            source UNINDEXED, title, body, tokenize = 'porter unicode61');
         DELETE FROM docs_fts;",
    )
    .expect("create docs_fts failed");

    let Ok(entries) = fs::read_dir(dir) else {
        eprintln!("warning: docs directory not found: {dir}");
        return 0;
    };

    let mut count = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let source = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for (title, body) in split_chunks(&text) {
            if conn
                .execute(
                    "INSERT INTO docs_fts(source, title, body) VALUES (?1, ?2, ?3)",
                    params![source, title, body],
                )
                .is_ok()
            {
                count += 1;
            }
        }
    }
    count
}

/// แบ่งข้อความเป็น chunk ตามหัวข้อ "## "
fn split_chunks(text: &str) -> Vec<(String, String)> {
    let mut chunks = Vec::new();
    let mut title: Option<String> = None;
    let mut body = String::new();

    let mut flush = |t: Option<String>, b: &str, out: &mut Vec<(String, String)>| {
        if let Some(t) = t {
            if !b.trim().is_empty() {
                out.push((t, b.trim().to_string()));
            }
        }
    };

    for line in text.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            flush(title.take(), &body, &mut chunks);
            title = Some(h.trim().to_string());
            body.clear();
        } else if title.is_some() {
            body.push_str(line);
            body.push('\n');
        }
    }
    flush(title.take(), &body, &mut chunks);
    chunks
}

/// แปลงคำค้นเป็น FTS5 query ที่ปลอดภัย: เหลือเฉพาะตัวอักษร/ตัวเลข ครอบด้วย "" แล้วเชื่อมด้วย OR
fn fts_query(q: &str) -> Option<String> {
    let terms: Vec<String> = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .take(12)
        .map(|t| format!("\"{t}\""))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

fn search_docs(conn: &Connection, args: &Value) -> Value {
    let q = args["query"].as_str().unwrap_or("");
    let Some(fts) = fts_query(q) else {
        return json!({ "error": "empty query" });
    };

    let mut stmt = match conn.prepare(
        "SELECT source, title, body FROM docs_fts
         WHERE docs_fts MATCH ?1 ORDER BY rank LIMIT 3",
    ) {
        Ok(s) => s,
        Err(_) => return json!({ "error": "database error" }),
    };

    let results: Vec<Value> = stmt
        .query_map(params![fts], |r| {
            let source: String = r.get(0)?;
            let title: String = r.get(1)?;
            let body: String = r.get(2)?;
            Ok(json!({
                "source": source,
                "section": title,
                "text": body.chars().take(1500).collect::<String>()
            }))
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default();

    if results.is_empty() {
        json!({ "results": [], "note": "no matching documentation found" })
    } else {
        json!({ "results": results })
    }
}