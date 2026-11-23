//! 云端朗读（F5，可选）。系统语音在前端用 Web Speech 播放，不经过这里。
//! Key 只在这里使用，不进 WebView、不写日志。返回 WAV 音频字节。

use crate::ai::{client, error_reason, http_error, net_error};
use crate::settings::{self, Accent, Provider, Settings};
use base64::Engine;
use serde_json::{Value, json};

fn accent_words(a: Accent) -> &'static str {
    match a {
        Accent::Us => "American",
        Accent::Uk => "British",
    }
}

/// Gemini 返回 24kHz / 16bit / 单声道的裸 PCM，补上 WAV 头才能播放。
fn pcm_to_wav(pcm: &[u8], sample_rate: u32) -> Vec<u8> {
    let (channels, bits) = (1u16, 16u16);
    let byte_rate = sample_rate * channels as u32 * bits as u32 / 8;
    let mut w = Vec::with_capacity(44 + pcm.len());
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&sample_rate.to_le_bytes());
    w.extend_from_slice(&byte_rate.to_le_bytes());
    w.extend_from_slice(&(channels * bits / 8).to_le_bytes());
    w.extend_from_slice(&bits.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(pcm);
    w
}

async fn check(resp: reqwest::Response) -> Result<reqwest::Response, String> {
    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        Ok(resp)
    } else {
        let body = resp.text().await.unwrap_or_default();
        Err(http_error(status, error_reason(&body)))
    }
}

pub async fn speak(s: &Settings, text: &str) -> Result<Vec<u8>, String> {
    let t = &s.tts;
    let provider = t.cloud;
    let key = settings::get_key(provider).ok_or_else(|| {
        let name = if provider == Provider::Gemini { "Gemini" } else { "OpenAI" };
        format!("云端朗读要用 {name} 的 API Key，请到「设置」里填入，或改用系统语音。")
    })?;
    let accent = accent_words(t.accent);
    let http = client();
    match provider {
        Provider::Gemini => {
            let resp = http
                .post(format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
                    t.gemini_model
                ))
                .header("x-goog-api-key", &key)
                .json(&json!({
                    "contents": [{ "parts": [{ "text": format!("Read aloud clearly, in a natural {accent} English accent:\n{text}") }] }],
                    "generationConfig": {
                        "responseModalities": ["AUDIO"],
                        "speechConfig": { "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": t.gemini_voice } } },
                    },
                }))
                .send()
                .await
                .map_err(net_error)?;
            let v: Value = check(resp).await?.json().await.map_err(net_error)?;
            let data = v
                .pointer("/candidates/0/content/parts/0/inlineData/data")
                .and_then(Value::as_str)
                .ok_or("云端语音没有返回音频，请重试。")?;
            let pcm = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| "云端语音返回的音频无法解析。".to_string())?;
            Ok(pcm_to_wav(&pcm, 24_000))
        }
        _ => {
            let resp = http
                .post(format!("{}/audio/speech", s.openai_base()))
                .bearer_auth(&key)
                .json(&json!({
                    "model": t.openai_model,
                    "voice": t.openai_voice,
                    "input": text,
                    "instructions": format!("Speak clearly at a natural pace with a {accent} English accent."),
                    "response_format": "wav",
                }))
                .send()
                .await
                .map_err(net_error)?;
            Ok(check(resp).await?.bytes().await.map_err(net_error)?.to_vec())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_valid() {
        let w = pcm_to_wav(&[0u8; 100], 24_000);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()), 136);
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 24_000);
        assert_eq!(&w[36..40], b"data");
        assert_eq!(w.len(), 144);
    }
}
