// 仅开发用：在普通浏览器里（没有 Tauri 后端）打开 http://localhost:1420/#panel 时，
// 用假数据模拟后端，方便调界面。生产构建不会加载这个文件。

const sample =
  "Hi team, just a quick reminder to circle back on the Q3 roadmap by EOD Friday. I'm OOO next week, so please loop in Sarah if anything is blocked.";
const reply =
  "大家好，提醒一下：请在周五下班前（EOD，End of Day）跟进（circle back，回头再处理/回复）第三季度路线图。我下周休假（OOO，Out of Office），如果有任何事情卡住了，请拉上 Sarah 一起沟通。";

let cbId = 0;
const callbacks = new Map<number, (data: unknown) => void>();

const handlers: Record<string, (args: any) => unknown> = {
  get_settings: () => ({
    provider: "gemini",
    gemini: { fast: "gemini-2.5-flash", strong: "gemini-2.5-pro" },
    openai: { fast: "gpt-4o-mini", strong: "gpt-4o" },
    anthropic: { fast: "claude-haiku-5-5", strong: "claude-sonnet-5-5" },
    openai_base_url: "https://api.openai.com/v1",
    tts: {
      engine: "system", cloud: "openai", openai_model: "gpt-4o-mini-tts", openai_voice: "alloy",
      gemini_model: "gemini-2.5-flash-preview-tts", gemini_voice: "Kore", rate: 1, accent: "us",
    },
  }),
  save_settings: () => null,
  has_api_key: () => false,
  set_api_key: () => null,
  last_grab: () => ({ id: 1, text: sample, error: null, tab: null }),
  copy_text: () => null,
  hide_panel: () => null,
  open_settings: () => null,
  trigger_permission: () => false,
  replace_text: () => null,
  speak_cloud: () => { throw "mock: 云端语音不可用"; },
  open_permission_settings: () => null,
  "plugin:event|listen": () => 1,
  "plugin:event|unlisten": () => null,
  run_feature: ({ onEvent }) => {
    const id: number = onEvent.id;
    const send = callbacks.get(id)!;
    const chunks = reply.match(/.{1,8}/gu) ?? [];
    let i = 0;
    const t = setInterval(() => {
      if (i < chunks.length) send({ index: i, message: { kind: "delta", text: chunks[i++] } });
      else {
        send({ index: i, message: { kind: "done" } });
        send({ index: i + 1, end: true });
        clearInterval(t);
      }
    }, 60);
    return null;
  },
};

(window as any).__TAURI_INTERNALS__ = {
  transformCallback(cb: (d: unknown) => void) {
    const id = ++cbId;
    callbacks.set(id, cb);
    return id;
  },
  unregisterCallback(id: number) {
    callbacks.delete(id);
  },
  async invoke(cmd: string, args: any) {
    const h = handlers[cmd];
    if (!h) throw new Error(`mock: ${cmd} not handled`);
    return h(args);
  },
  metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
};
(window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
export {};
