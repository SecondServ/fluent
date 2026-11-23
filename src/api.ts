import { Channel, invoke } from "@tauri-apps/api/core";

export type Provider = "gemini" | "openai" | "anthropic";
/** 面板上的五个功能页。 */
export type Tab = "translate" | "summarize" | "rewrite" | "reply" | "read";
/** 后端 AI 功能（朗读页的音标用 ipa）。 */
export type AiFeature = "translate" | "summarize" | "rewrite" | "reply" | "ipa";
export type Tone = "formal" | "neutral" | "friendly" | "concise";
export type Stance = "agree" | "decline" | "info" | "later" | "thanks";

export interface RunRequest {
  feature: AiFeature;
  text: string;
  tone?: Tone;
  explain?: boolean;
  intent?: string;
  stance?: Stance | null;
}

export interface Tts {
  engine: "system" | "cloud";
  cloud: "openai" | "gemini";
  openai_model: string;
  openai_voice: string;
  gemini_model: string;
  gemini_voice: string;
  rate: number;
  accent: "us" | "uk";
}

export interface Models {
  fast: string;
  strong: string;
}

export interface Settings {
  provider: Provider;
  gemini: Models;
  openai: Models;
  anthropic: Models;
  openai_base_url: string;
  tts: Tts;
}

export interface GrabResult {
  id: number;
  text: string | null;
  error: string | null;
  /** 点了长条上的哪个功能；null = 保持上次的页。 */
  tab: Tab | null;
}

export type StreamEvent =
  | { kind: "delta"; text: string }
  | { kind: "done" }
  | { kind: "error"; message: string };

export const PROVIDERS: { id: Provider; name: string; hint: string }[] = [
  { id: "gemini", name: "Google Gemini", hint: "推荐。请用付费 API Key：免费 AI Studio Key 的数据可能被用于训练。" },
  { id: "openai", name: "OpenAI 兼容", hint: "OpenAI，或任何 OpenAI 兼容的服务（改 Base URL）。" },
  { id: "anthropic", name: "Anthropic Claude", hint: "Claude API Key（console.anthropic.com）。" },
];

export const isMac = navigator.userAgent.includes("Mac");

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  hasApiKey: (provider: Provider) => invoke<boolean>("has_api_key", { provider }),
  setApiKey: (provider: Provider, key: string) => invoke<void>("set_api_key", { provider, key }),
  lastGrab: () => invoke<GrabResult | null>("last_grab"),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  hidePanel: () => invoke<void>("hide_panel"),
  openSettings: () => invoke<void>("open_settings"),
  triggerPermission: () => invoke<boolean>("trigger_permission"),
  openPermissionSettings: () => invoke<void>("open_permission_settings"),
  runFeature: (req: RunRequest, onEvent: (e: StreamEvent) => void) => {
    const ch = new Channel<StreamEvent>();
    ch.onmessage = onEvent;
    return invoke<void>("run_feature", { req, onEvent: ch });
  },
  /** 云端朗读，返回 WAV 音频。 */
  speakCloud: (text: string) => invoke<ArrayBuffer>("speak_cloud", { text }),
  /** 切回原程序并粘贴，替换原来选中的文字。 */
  replaceText: (text: string) => invoke<void>("replace_text", { text }),
};
