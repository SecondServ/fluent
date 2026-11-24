//! 功能定义与 Prompt 模板（SPEC §6）。纯逻辑，不碰 Tauri。

use serde::Deserialize;

pub const SYSTEM: &str = "You are a writing assistant for a non-native English speaker working in an international company. Output only the result itself — no preamble, no explanations, no quotes. The result must be ready to copy and use.";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Feature {
    Translate,
    Summarize,
    Rewrite,
    Reply,
    /// 朗读页上给单词 / 短语标音标。
    Ipa,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    #[default]
    Formal,
    Neutral,
    Friendly,
    Concise,
}

impl Tone {
    fn en(self) -> &'static str {
        match self {
            Tone::Formal => "formal",
            Tone::Neutral => "neutral",
            Tone::Friendly => "friendly",
            Tone::Concise => "concise",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Stance {
    Agree,
    Decline,
    Info,
    Later,
    Thanks,
}

impl Stance {
    fn en(self) -> &'static str {
        match self {
            Stance::Agree => "agree",
            Stance::Decline => "decline politely",
            Stance::Info => "ask for more info",
            Stance::Later => "will follow up later",
            Stance::Thanks => "thank and confirm",
        }
    }
}

/// 前端发来的一次请求。各功能只用到自己的字段，其余忽略。
#[derive(Clone, Debug, Deserialize)]
pub struct Request {
    pub feature: Feature,
    pub text: String,
    #[serde(default)]
    pub tone: Tone,
    /// 改写：是否在结果后附「改了什么」。
    #[serde(default)]
    pub explain: bool,
    /// 回复：我的回复意图（可中文、可空）。
    #[serde(default)]
    pub intent: String,
    /// 回复：立场（可不选）。
    #[serde(default)]
    pub stance: Option<Stance>,
}

pub struct Prompt {
    pub system: &'static str,
    pub user: String,
    pub temperature: f32,
}

/// 改写「改了什么」和正文之间的分隔行；前端据此拆开，复制 / 替换只取正文。
pub const EXPLAIN_SEPARATOR: &str = "---";

pub fn build(r: &Request) -> Prompt {
    let text = &r.text;
    let (user, temperature) = match r.feature {
        Feature::Translate => (
            format!(
                "Translate the text below.\n\
                 - If it is mainly English, translate into Simplified Chinese; if mainly Chinese, translate into natural English.\n\
                 - Be faithful and complete: keep every number, date, name, link and requirement.\n\
                 - For workplace jargon or acronyms (e.g. EOD, OOO, circle back), add a short Chinese explanation in parentheses the first time they appear.\n\
                 Text:\n{text}"
            ),
            0.2,
        ),
        Feature::Summarize => (
            format!(
                "Summarize the message below in Simplified Chinese using EXACTLY this format:\n\
                 一句话：<one sentence: what this message is about>\n\
                 要点：\n\
                 • <key point>（最多 5 条，只保留真正重要的）\n\
                 要我做的：\n\
                 • <action requested from the reader, with deadline if any>   （若没有，写「无需行动」）\n\
                 Do not invent anything not in the message.\n\
                 Message:\n{text}"
            ),
            0.2,
        ),
        Feature::Rewrite => {
            let explain = if r.explain {
                format!(
                    "\nAfter the rewrite, output a line \"{EXPLAIN_SEPARATOR}\" and then 2–4 bullets in Simplified Chinese explaining the most useful changes (word choice, grammar, tone)."
                )
            } else {
                String::new()
            };
            (
                format!(
                    "Rewrite my draft into clear, natural, native-sounding English for a workplace message.\n\
                     - Tone: {}\n\
                     - The draft may contain Chinese or broken English — understand my intent and express it in English.\n\
                     - Keep my meaning; do not add facts, promises or details I did not write.\n\
                     - Keep the same format (email stays an email, chat stays short).{explain}\n\
                     Draft:\n{text}",
                    r.tone.en()
                ),
                0.4,
            )
        }
        Feature::Reply => {
            let intent = r.intent.trim();
            let intent = if intent.is_empty() { "(none — write a sensible, brief reply)" } else { intent };
            let stance = r.stance.map_or("(not specified — follow my intent)", Stance::en);
            (
                format!(
                    "Write an English reply to the message below.\n\
                     - My intent (may be in Chinese, may be empty): {intent}\n\
                     - Stance: {stance}\n\
                     - Tone: {}\n\
                     - Match the format of the original (email → email with greeting/sign-off; chat → short message). Do not invent facts, dates or commitments beyond my intent.\n\
                     Output only the reply body.\n\
                     Message:\n{text}",
                    r.tone.en()
                ),
                0.5,
            )
        }
        Feature::Ipa => (
            format!(
                "Give the IPA (US and UK) for the word or short phrase below, plus one tip in Simplified Chinese on how to pronounce it. Format: \"US /…/  UK /…/\\n<tip>\"\n{text}"
            ),
            0.0,
        ),
    };
    Prompt { system: SYSTEM, user, temperature }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(json: &str) -> Request {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn translate_embeds_text() {
        let p = build(&req(r#"{"feature":"translate","text":"Please circle back by EOD."}"#));
        assert!(p.user.ends_with("Text:\nPlease circle back by EOD."));
        assert!(p.user.contains("Simplified Chinese"));
        assert_eq!(p.system, SYSTEM);
    }

    #[test]
    fn summarize_has_fixed_format() {
        let p = build(&req(r#"{"feature":"summarize","text":"x"}"#));
        assert!(p.user.contains("一句话：") && p.user.contains("要我做的："));
    }

    #[test]
    fn rewrite_tone_and_explain() {
        let p = build(&req(r#"{"feature":"rewrite","text":"pls check","tone":"friendly","explain":true}"#));
        assert!(p.user.contains("Tone: friendly"));
        assert!(p.user.contains("\"---\""));
        let p = build(&req(r#"{"feature":"rewrite","text":"pls check"}"#));
        assert!(p.user.contains("Tone: formal"));
        assert!(!p.user.contains("\"---\""));
    }

    #[test]
    fn reply_intent_and_stance() {
        let p = build(&req(r#"{"feature":"reply","text":"Can you join?","intent":"可以，周三","stance":"agree"}"#));
        assert!(p.user.contains("My intent (may be in Chinese, may be empty): 可以，周三"));
        assert!(p.user.contains("Stance: agree"));
        assert!(p.user.ends_with("Message:\nCan you join?"));
        let p = build(&req(r#"{"feature":"reply","text":"Hi","intent":"  "}"#));
        assert!(p.user.contains("(none"));
        assert!(p.user.contains("(not specified"));
    }
}
