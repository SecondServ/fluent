import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, GrabResult, isMac, RunRequest, Stance, Tab, Tone } from "./api";
import Speaker from "./Speaker";

type Status = "idle" | "loading" | "streaming" | "done" | "error";
type Out = { text: string; status: Status; error: string | null; sig: string };

const TABS: { id: Tab; name: string }[] = [
  { id: "translate", name: "翻译" },
  { id: "summarize", name: "总结" },
  { id: "rewrite", name: "改写" },
  { id: "reply", name: "回复" },
  { id: "read", name: "朗读" },
];
const TONES: { id: Tone; name: string }[] = [
  { id: "formal", name: "正式" },
  { id: "neutral", name: "中性" },
  { id: "friendly", name: "友好" },
  { id: "concise", name: "简洁" },
];
const STANCES: { id: Stance; name: string }[] = [
  { id: "agree", name: "同意" },
  { id: "decline", name: "婉拒" },
  { id: "info", name: "要更多信息" },
  { id: "later", name: "稍后跟进" },
  { id: "thanks", name: "感谢确认" },
];
const LOADING: Record<string, string> = {
  translate: "正在翻译",
  summarize: "正在总结",
  rewrite: "正在改写",
  reply: "正在写回复",
};
const mod = isMac ? "⌘" : "Ctrl+";
/** 关面板时让朗读停下（朗读页监听这个事件）。 */
const stopSpeech = () => window.dispatchEvent(new Event("fluent:stop"));

// 面板记住上次用的功能页、语气等（只存在本机浏览器存储，存不了也不影响使用）。
function load<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : (JSON.parse(v) as T);
  } catch {
    return fallback;
  }
}
function store(key: string, v: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(v));
  } catch {
    /* ignore */
  }
}

/** 改写结果里「---」之后是「改了什么」；复制 / 替换只取正文。 */
function splitExplain(text: string): [string, string | null] {
  const m = text.match(/\n\s*-{3,}\s*\n/);
  if (!m || m.index === undefined) return [text, null];
  return [text.slice(0, m.index).trimEnd(), text.slice(m.index + m[0].length).trim()];
}

/** 是不是单词或短语（给朗读页标音标）。 */
function isShortEnglish(t: string) {
  const s = t.trim();
  return s.length <= 40 && /^[A-Za-z][A-Za-z' .-]*$/.test(s) && s.split(/\s+/).length <= 4;
}

export default function Panel() {
  const [grab, setGrab] = useState<GrabResult | null>(null);
  const [tab, setTabState] = useState<Tab>(() => load("tab", "translate"));
  const [tone, setTone] = useState<Tone>(() => load("tone", "formal"));
  const [explain, setExplain] = useState<boolean>(() => load("explain", false));
  const [intent, setIntent] = useState("");
  const [stance, setStance] = useState<Stance | null>(null);
  const [outs, setOuts] = useState<Record<string, Out>>({});
  /** 改写 / 回复的结果可以在面板里直接改。 */
  const [edits, setEdits] = useState<Record<string, string>>({});
  const [copied, setCopied] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const runIds = useRef<Record<string, number>>({});
  const lastGrabId = useRef(0);
  const intentBox = useRef<HTMLTextAreaElement>(null);

  const text = grab?.text ?? null;

  const setTab = (t: Tab) => {
    setTabState(t);
    store("tab", t);
    setActionError(null);
  };

  const run = useCallback((slot: string, req: RunRequest) => {
    const id = (runIds.current[slot] ?? 0) + 1;
    runIds.current[slot] = id;
    const sig = JSON.stringify(req);
    const set = (patch: Partial<Out> | ((o: Out) => Partial<Out>)) =>
      setOuts((all) => {
        const cur = all[slot];
        if (!cur || runIds.current[slot] !== id) return all;
        return { ...all, [slot]: { ...cur, ...(typeof patch === "function" ? patch(cur) : patch) } };
      });
    setOuts((all) => ({ ...all, [slot]: { text: "", status: "loading", error: null, sig } }));
    setEdits((e) => {
      const { [slot]: _, ...rest } = e;
      return rest;
    });
    setCopied(false);
    setActionError(null);
    api
      .runFeature(req, (e) => {
        if (e.kind === "delta") set((o) => ({ status: "streaming", text: o.text + e.text }));
        else if (e.kind === "done") set({ status: "done" });
        else set({ status: "error", error: e.message });
      })
      .catch((err) => set({ status: "error", error: String(err) }));
  }, []);

  // 当前页该发的请求（回复页要等用户点「生成」，不自动发）。
  const autoReq = useCallback((): RunRequest | null => {
    if (!text) return null;
    switch (tab) {
      case "translate":
      case "summarize":
        return { feature: tab, text };
      case "rewrite":
        return { feature: "rewrite", text, tone, explain };
      default:
        return null;
    }
  }, [tab, text, tone, explain]);

  useEffect(() => {
    const req = autoReq();
    if (req && outs[req.feature]?.sig !== JSON.stringify(req)) run(req.feature, req);
    if (tab === "read" && text && isShortEnglish(text)) {
      const ipa: RunRequest = { feature: "ipa", text: text.trim() };
      if (outs.ipa?.sig !== JSON.stringify(ipa)) run("ipa", ipa);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoReq, tab, text]);

  const generateReply = useCallback(() => {
    if (text) run("reply", { feature: "reply", text, tone, intent, stance });
  }, [run, text, tone, intent, stance]);

  const handleGrab = useCallback((g: GrabResult) => {
    if (g.id === lastGrabId.current) return;
    lastGrabId.current = g.id;
    // 新的一段文字：作废所有进行中的请求，清空结果。
    for (const k of Object.keys(runIds.current)) runIds.current[k]++;
    setOuts({});
    setEdits({});
    setIntent("");
    setStance(null);
    setActionError(null);
    if (g.tab) setTab(g.tab);
    setGrab(g);
  }, []);

  useEffect(() => {
    api.lastGrab().then((g) => g && handleGrab(g));
    const un = listen<GrabResult>("grab", (e) => handleGrab(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, [handleGrab]);

  useEffect(() => {
    if (tab === "reply") intentBox.current?.focus();
  }, [tab, grab]);

  const out = tab === "read" ? undefined : outs[tab];
  const busy = out?.status === "loading" || out?.status === "streaming";
  const writing = tab === "rewrite" || tab === "reply";
  const [body, notes] = out ? (tab === "rewrite" ? splitExplain(out.text) : [out.text, null]) : ["", null];
  const finalText = writing ? (edits[tab] ?? body) : body;

  const copy = useCallback(async () => {
    if (!finalText) return;
    await api.copyText(finalText);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }, [finalText]);

  const replace = useCallback(async () => {
    if (!finalText || !writing || busy) return;
    setActionError(null);
    try {
      await api.replaceText(finalText);
    } catch (e) {
      setActionError(String(e));
    }
  }, [finalText, writing, busy]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement;
      const typing = el.tagName === "TEXTAREA" || el.tagName === "INPUT";
      const cmd = e.metaKey || e.ctrlKey;
      if (e.key === "Escape") {
        stopSpeech();
        api.hidePanel();
      } else if (!typing && !cmd && /^[1-5]$/.test(e.key)) {
        e.preventDefault(); // 别让数字落进随后获得焦点的输入框
        setTab(TABS[Number(e.key) - 1].id);
      } else if (cmd && e.key === "Enter") {
        e.preventDefault();
        if (el === intentBox.current) generateReply();
        else replace();
      } else if (cmd && e.key === "c" && !typing && !window.getSelection()?.toString()) {
        // 面板里没有选中文字时，⌘C / Ctrl+C 复制整段结果
        e.preventDefault();
        copy();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [copy, replace, generateReply]);

  const retry = () => {
    if (tab === "reply") generateReply();
    else {
      const req = autoReq();
      if (req) run(req.feature, req);
    }
  };

  const toneChips = (
    <div className="chips">
      {TONES.map((t) => (
        <button
          key={t.id}
          className={tone === t.id ? "chip on" : "chip"}
          onClick={() => {
            setTone(t.id);
            store("tone", t.id);
          }}
        >
          {t.name}
        </button>
      ))}
    </div>
  );

  return (
    <div className="panel">
      <header className="panel-head" data-tauri-drag-region>
        <nav className="tabs">
          {TABS.map((t, i) => (
            <button
              key={t.id}
              className={tab === t.id ? "tab on" : "tab"}
              title={`${t.name}（${i + 1}）`}
              onClick={() => setTab(t.id)}
            >
              {t.name}
            </button>
          ))}
        </nav>
        <span className="spacer" data-tauri-drag-region />
        <button className="icon" title="设置" onClick={() => api.openSettings()}>
          ⚙
        </button>
        <button
          className="icon"
          title="关闭 (Esc)"
          onClick={() => {
            stopSpeech();
            api.hidePanel();
          }}
        >
          ✕
        </button>
      </header>

      {text && (
        <div className="source" title={text}>
          {text}
        </div>
      )}

      {text && tab === "rewrite" && (
        <div className="opts row">
          <span className="label">语气</span>
          {toneChips}
          <span className="spacer" />
          <label className="check">
            <input
              type="checkbox"
              checked={explain}
              onChange={(e) => {
                setExplain(e.target.checked);
                store("explain", e.target.checked);
              }}
            />
            显示改了什么
          </label>
        </div>
      )}

      {text && tab === "reply" && (
        <div className="opts reply-opts">
          <textarea
            ref={intentBox}
            rows={2}
            placeholder={`想怎么回？用中文写就行，可以留空。${mod}Enter 生成`}
            value={intent}
            onChange={(e) => setIntent(e.target.value)}
          />
          <div className="row">
            <span className="label">立场</span>
            <div className="chips">
              {STANCES.map((s) => (
                <button
                  key={s.id}
                  className={stance === s.id ? "chip on" : "chip"}
                  onClick={() => setStance(stance === s.id ? null : s.id)}
                >
                  {s.name}
                </button>
              ))}
            </div>
          </div>
          <div className="row">
            <span className="label">语气</span>
            {toneChips}
            <span className="spacer" />
            <button className="primary" disabled={busy} onClick={generateReply}>
              {out ? "重新生成" : "生成回复"}
            </button>
          </div>
        </div>
      )}

      <main className="result">
        {!text && grab?.error && (
          <div className="error">
            <p>{grab.error}</p>
            {grab.error.includes("设置") && <button onClick={() => api.openSettings()}>打开设置</button>}
          </div>
        )}

        {text && tab === "read" && (
          <>
            <Speaker text={text} playKey={grab!.id} />
            {outs.ipa && outs.ipa.status !== "error" && (
              <div className="ipa">{outs.ipa.text || <span className="dots">查音标</span>}</div>
            )}
          </>
        )}

        {out?.status === "loading" && <div className="dots">{LOADING[tab]}</div>}
        {out &&
          out.text &&
          (writing ? (
            <textarea
              className="edit"
              value={finalText}
              readOnly={busy}
              onChange={(e) => setEdits((all) => ({ ...all, [tab]: e.target.value }))}
            />
          ) : (
            <div className="text">{body}</div>
          ))}
        {notes && <div className="notes">{notes}</div>}
        {out?.status === "error" && (
          <div className="error">
            <p>{out.error}</p>
            <div className="row">
              {out.error?.includes("设置") && <button onClick={() => api.openSettings()}>打开设置</button>}
              <button onClick={retry}>重试</button>
            </div>
          </div>
        )}
        {actionError && (
          <div className="error">
            <p>{actionError}</p>
          </div>
        )}
      </main>

      {tab !== "read" && (
        <footer className="panel-foot">
          <span className="hint">{busy ? "生成中…" : "1–5 切换 · Esc 关闭"}</span>
          <span className="spacer" />
          {text && tab !== "reply" && (
            <button disabled={busy} onClick={retry}>
              重新生成
            </button>
          )}
          <button className={writing ? "" : "primary"} disabled={!finalText || busy} onClick={copy}>
            {copied ? "已复制 ✓" : `复制 ${mod}C`}
          </button>
          {writing && (
            <button className="primary" disabled={!finalText || busy} onClick={replace} title="切回原程序，用结果替换选中的文字（不会发送）">
              替换原文 {mod}↵
            </button>
          )}
        </footer>
      )}
    </div>
  );
}
