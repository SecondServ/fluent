//! 非敏感设置（厂商、模型名、Base URL）存 JSON；API Key 只存系统凭据库。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const KEYRING_SERVICE: &str = "com.fluent.desktop";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Gemini,
    Openai,
    Anthropic,
}

impl Provider {
    pub fn key_name(self) -> &'static str {
        match self {
            Provider::Gemini => "gemini",
            Provider::Openai => "openai",
            Provider::Anthropic => "anthropic",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Models {
    /// 所有功能共用一个模型（求快）。旧配置里叫 fast，另有一个已不用的 strong 字段，读取时忽略。
    #[serde(alias = "fast")]
    pub model: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TtsEngine {
    /// 系统自带语音：免费、离线。
    System,
    /// 云端语音（OpenAI / Gemini），更自然，按量付费。
    Cloud,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    Us,
    Uk,
}

/// 朗读设置。云端语音用对应厂商已保存的 Key。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Tts {
    pub engine: TtsEngine,
    /// 云端用哪家：只支持 gemini / openai。
    pub cloud: Provider,
    pub openai_model: String,
    pub openai_voice: String,
    pub gemini_model: String,
    pub gemini_voice: String,
    pub rate: f32,
    pub accent: Accent,
}

impl Default for Tts {
    fn default() -> Self {
        Tts {
            engine: TtsEngine::System,
            cloud: Provider::Openai,
            openai_model: "gpt-4o-mini-tts".into(),
            openai_voice: "alloy".into(),
            gemini_model: "gemini-2.5-flash-preview-tts".into(),
            gemini_voice: "Kore".into(),
            rate: 1.0,
            accent: Accent::Us,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub provider: Provider,
    pub gemini: Models,
    pub openai: Models,
    pub anthropic: Models,
    pub openai_base_url: String,
    pub tts: Tts,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            provider: Provider::Gemini,
            gemini: Models { model: "gemini-2.5-flash".into() },
            openai: Models { model: "gpt-4o-mini".into() },
            anthropic: Models { model: "claude-haiku-5-5".into() },
            openai_base_url: "https://api.openai.com/v1".into(),
            tts: Tts::default(),
        }
    }
}

impl Settings {
    /// OpenAI 兼容服务的 Base URL（去掉结尾的 /，空则用官方地址）。
    pub fn openai_base(&self) -> &str {
        let base = self.openai_base_url.trim().trim_end_matches('/');
        if base.is_empty() { "https://api.openai.com/v1" } else { base }
    }

    pub fn models(&self) -> &Models {
        match self.provider {
            Provider::Gemini => &self.gemini,
            Provider::Openai => &self.openai,
            Provider::Anthropic => &self.anthropic,
        }
    }

    pub fn load(path: &PathBuf) -> Settings {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, s).map_err(|e| e.to_string())
    }
}

fn entry(p: Provider) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, p.key_name()).map_err(|e| format!("凭据库不可用：{e}"))
}

pub fn get_key(p: Provider) -> Option<String> {
    entry(p).ok()?.get_password().ok().filter(|k| !k.is_empty())
}

pub fn set_key(p: Provider, key: &str) -> Result<(), String> {
    let e = entry(p)?;
    if key.trim().is_empty() {
        return match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(format!("删除 Key 失败：{err}")),
        };
    }
    e.set_password(key.trim()).map_err(|err| format!("保存 Key 失败：{err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_two_tier_settings_still_load() {
        let m: Models = serde_json::from_str(r#"{"fast":"gemini-2.5-flash","strong":"gemini-2.5-pro"}"#).unwrap();
        assert_eq!(m.model, "gemini-2.5-flash");
    }
}
