use serde_json::{json, Value};
use std::time::Duration;

pub enum LlmError {
    RateLimited,
    Http(String),
}

pub struct Llm {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl Llm {
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("GEMINI_API_KEY").ok()?;
        let model = std::env::var("GEMINI_MODEL").ok()?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .ok()?;
        Some(Self { client, api_key, model })
    }

    async fn generate(
        &self,
        system: &str,
        contents: &[Value],
        tools: &Value,
    ) -> Result<Value, LlmError> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            self.model
        );
        let body = json!({
            "systemInstruction": { "parts": [{ "text": system }] },
            "contents": contents,
            "tools": tools
        });

        let resp = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;

        let status = resp.status();
        if status.as_u16() == 429 {
            return Err(LlmError::RateLimited);
        }
        let v: Value = resp
            .json()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;
        if !status.is_success() {
            let msg = v["error"]["message"].as_str().unwrap_or("unknown error");
            return Err(LlmError::Http(format!("{status}: {msg}")));
        }
        Ok(v)
    }

    /// ถาม LLM โดยให้เรียก tool ได้ไม่เกิน max_rounds รอบ
    pub async fn ask_with_tools<F>(
        &self,
        system: &str,
        user: &str,
        tools: &Value,
        mut run_tool: F,
        max_rounds: usize,
    ) -> Result<String, LlmError>
    where
        F: FnMut(&str, &Value) -> Value + Send,
    {
        let mut contents = vec![json!({ "role": "user", "parts": [{ "text": user }] })];

        for round in 0..=max_rounds {
            let v = self.generate(system, &contents, tools).await?;
            let content = v["candidates"][0]["content"].clone();
            let parts: Vec<Value> = content["parts"].as_array().cloned().unwrap_or_default();

            let calls: Vec<Value> = parts
                .iter()
                .filter(|p| p.get("functionCall").is_some())
                .cloned()
                .collect();

            // ไม่ขอเรียก tool แล้ว -> รวมข้อความคำตอบ
            if calls.is_empty() {
                let text: String = parts
                    .iter()
                    .filter(|p| p["thought"].as_bool() != Some(true))
                    .filter_map(|p| p["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("");
                if text.is_empty() {
                    return Err(LlmError::Http("empty response".into()));
                }
                return Ok(text);
            }

            if round == max_rounds {
                return Err(LlmError::Http("too many tool rounds".into()));
            }

            // ส่งกลับ content ของโมเดลตามเดิม (จำเป็นสำหรับโมเดลที่ใช้ thought signature)
            contents.push(content);

            let responses: Vec<Value> = calls
                .iter()
                .take(4)
                .map(|c| {
                    let name = c["functionCall"]["name"].as_str().unwrap_or("");
                    let args = &c["functionCall"]["args"];
                    let result = run_tool(name, args);
                    json!({ "functionResponse": { "name": name, "response": { "result": result } } })
                })
                .collect();
            contents.push(json!({ "role": "user", "parts": responses }));
        }
        Err(LlmError::Http("unreachable".into()))
    }
}