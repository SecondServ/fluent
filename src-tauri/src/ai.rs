//! 三家大模型的流式调用（SSE）。Key 只在这里使用，不进 WebView，不写日志。

use crate::prompts::Prompt;
use crate::settings::{Provider, Settings};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::time::Duration;

const ANTHROPIC_VERSION: &str = "2023-06-01";

pub(crate) fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(60))
        .build()
        .expect("http client")
}

/// 按 HTTP 状态码给出中文友好提示；`reason` 取自 API 返回的错误信息（不含用户文本）。
pub(crate) fn http_error(status: u16, reason: Option<String>) -> String {
    let reason = reason.filter(|r| !r.is_empty());
    match status {
        429 => "请求过于频繁或额度超限（HTTP 429），请稍后再试。".into(),
        401 | 403 => "API Key 无效或没有权限，请到「设置」检查 Key。".into(),
        _ => match reason {
            Some(r) => format!("请求失败（HTTP {status}）：{r}"),
            None => format!("请求失败（HTTP {status}），请稍后重试。"),
        },
    }
}

pub(crate) fn error_reason(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    v.pointer("/error/message")
        .or_else(|| v.pointer("/0/error/message"))
        .and_then(|m| m.as_str())
        .map(|s| s.chars().take(200).collect())
}

pub(crate) fn net_error(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "请求超时，请检查网络后重试。".into()
    } else if e.is_connect() {
        "连不上服务器，请检查网络（或代理）后重试。".into()
    } else {
        "网络错误，请重试。".into()
    }
}

/// 从一个 SSE `data:` JSON 里取出增量文本。
fn delta_text(provider: Provider, v: &Value) -> Option<String> {
    let s = match provider {
        Provider::Gemini => v
            .pointer("/candidates/0/content/parts")?
            .as_array()?
            .iter()
            .filter(|p| !p.get("thought").and_then(Value::as_bool).unwrap_or(false))
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<String>(),
        Provider::Openai => v.pointer("/choices/0/delta/content")?.as_str()?.to_string(),
        Provider::Anthropic => {
            if v.get("type")?.as_str()? != "content_block_delta" {
                return None;
            }
            v.pointer("/delta/text")?.as_str()?.to_string()
        }
    };
    (!s.is_empty()).then_some(s)
}

fn stream_error(provider: Provider, v: &Value) -> Option<String> {
    match provider {
        Provider::Anthropic if v.get("type")?.as_str()? == "error" => Some(
            v.pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("模型返回错误")
                .to_string(),
        ),
        _ => v.pointer("/error/message").and_then(Value::as_str).map(String::from),
    }
}

fn build_request(
    http: &reqwest::Client,
    settings: &Settings,
    key: &str,
    p: &Prompt,
    gemini_thinking_off: bool,
) -> reqwest::RequestBuilder {
    let model = &settings.models().model;
    match settings.provider {
        Provider::Gemini => {
            let mut gen_cfg = json!({ "temperature": p.temperature });
            if gemini_thinking_off {
                gen_cfg["thinkingConfig"] = json!({ "thinkingBudget": 0 });
            }
            http.post(format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent?alt=sse"
            ))
            .header("x-goog-api-key", key)
            .json(&json!({
                "systemInstruction": { "parts": [{ "text": p.system }] },
                "contents": [{ "role": "user", "parts": [{ "text": p.user }] }],
                "generationConfig": gen_cfg,
            }))
        }
        Provider::Openai => {
            http.post(format!("{}/chat/completions", settings.openai_base()))
                .bearer_auth(key)
                .json(&json!({
                    "model": model,
                    "stream": true,
                    "temperature": p.temperature,
                    "messages": [
                        { "role": "system", "content": p.system },
                        { "role": "user", "content": p.user },
                    ],
                }))
        }
        Provider::Anthropic => http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&json!({
                "model": model,
                "max_tokens": 4096,
                "stream": true,
                "temperature": p.temperature,
                "system": p.system,
                "messages": [{ "role": "user", "content": p.user }],
            })),
    }
}

/// 流式生成；每收到一段增量文本就调用 `on_delta`。返回 Err 时为中文错误提示。
pub async fn stream(
    settings: &Settings,
    prompt: &Prompt,
    mut on_delta: impl FnMut(String),
) -> Result<(), String> {
    let key = crate::settings::get_key(settings.provider)
        .ok_or_else(|| "还没有填 API Key。请到「设置」里填入。".to_string())?;
    let http = client();

    let mut resp = build_request(&http, settings, &key, prompt, true)
        .send()
        .await
        .map_err(net_error)?;
    // 某些 Gemini 模型不允许关闭思考（thinkingBudget=0 → 400），去掉后重试一次。
    if settings.provider == Provider::Gemini && resp.status().as_u16() == 400 {
        resp = build_request(&http, settings, &key, prompt, false)
            .send()
            .await
            .map_err(net_error)?;
    }
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        let body = resp.text().await.unwrap_or_default();
        return Err(http_error(status, error_reason(&body)));
    }

    let mut buf: Vec<u8> = Vec::new();
    let mut got_any = false;
    let mut body = resp.bytes_stream();
    while let Some(chunk) = body.next().await {
        buf.extend_from_slice(&chunk.map_err(net_error)?);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            let Some(data) = line.trim().strip_prefix("data:") else { continue };
            let data = data.trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
            if let Some(err) = stream_error(settings.provider, &v) {
                return Err(format!("模型返回错误：{err}"));
            }
            if let Some(t) = delta_text(settings.provider, &v) {
                got_any = true;
                on_delta(t);
            }
        }
    }
    if got_any { Ok(()) } else { Err("模型没有返回内容，请重试。".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_deltas_for_each_provider() {
        let g = json!({"candidates":[{"content":{"parts":[{"text":"思考","thought":true},{"text":"你好"}]}}]});
        assert_eq!(delta_text(Provider::Gemini, &g).as_deref(), Some("你好"));

        let o = json!({"choices":[{"delta":{"content":"Hi"}}]});
        assert_eq!(delta_text(Provider::Openai, &o).as_deref(), Some("Hi"));

        let a = json!({"type":"content_block_delta","delta":{"type":"text_delta","text":"Yo"}});
        assert_eq!(delta_text(Provider::Anthropic, &a).as_deref(), Some("Yo"));
        let ping = json!({"type":"ping"});
        assert_eq!(delta_text(Provider::Anthropic, &ping), None);
    }

    #[test]
    fn maps_http_errors() {
        assert!(http_error(429, None).contains("429"));
        assert!(http_error(401, Some("bad".into())).contains("Key"));
        assert!(http_error(404, Some("model not found".into())).contains("model not found"));
        assert_eq!(
            error_reason(r#"{"error":{"message":"nope"}}"#).as_deref(),
            Some("nope")
        );
    }
}
