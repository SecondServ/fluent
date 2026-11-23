import { useCallback, useEffect, useRef, useState } from "react";
import { api, Settings, Tts } from "./api";

type Play = "idle" | "loading" | "playing" | "paused";

const RATES = [0.75, 1, 1.25];

/** 系统里最合适的英文语音：按口音挑，优先增强版 / 高质量的。 */
function pickVoice(accent: Tts["accent"]): SpeechSynthesisVoice | null {
  const voices = window.speechSynthesis?.getVoices() ?? [];
  const lang = accent === "uk" ? "en-gb" : "en-us";
  const match = voices.filter((v) => v.lang.toLowerCase().replace("_", "-") === lang);
  const pool = match.length ? match : voices.filter((v) => v.lang.toLowerCase().startsWith("en"));
  const score = (v: SpeechSynthesisVoice) =>
    (/premium|enhanced|natural|neural/i.test(v.name) ? 2 : 0) + (v.localService ? 1 : 0);
  return pool.sort((a, b) => score(b) - score(a))[0] ?? null;
}

/** 朗读页：系统语音（Web Speech）或云端语音。 */
/** `playKey` 每次取到新文字时变化：变了就自动读一遍。 */
export default function Speaker({ text, playKey }: { text: string; playKey: number }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [play, setPlay] = useState<Play>("idle");
  const [error, setError] = useState<string | null>(null);
  const [voiceName, setVoiceName] = useState<string | null>(null);
  const audio = useRef<HTMLAudioElement | null>(null);
  /** 云端音频缓存：同一段文字、同一口音不重复收费。 */
  const cache = useRef<{ sig: string; url: string } | null>(null);
  const session = useRef(0);

  useEffect(() => {
    api.getSettings().then(setSettings);
  }, []);

  const tts = settings?.tts;

  const stop = useCallback(() => {
    session.current++;
    window.speechSynthesis?.cancel();
    audio.current?.pause();
    audio.current = null;
    setPlay("idle");
  }, []);

  const start = useCallback(async () => {
    if (!tts) return;
    stop();
    setError(null);
    const my = ++session.current;
    if (tts.engine === "system") {
      const synth = window.speechSynthesis;
      if (!synth) {
        setError("这个系统不支持系统语音，请到设置里改用云端语音。");
        return;
      }
      const voice = pickVoice(tts.accent);
      if (!voice) {
        setError("系统里没有英文语音。可以在系统设置里下载英文语音，或改用云端语音。");
        return;
      }
      setVoiceName(voice.name);
      const u = new SpeechSynthesisUtterance(text);
      u.voice = voice;
      u.lang = voice.lang;
      u.rate = tts.rate;
      u.onend = () => my === session.current && setPlay("idle");
      u.onerror = () => my === session.current && setPlay("idle");
      synth.speak(u);
      setPlay("playing");
      return;
    }
    // 云端
    setVoiceName(null);
    const sig = `${tts.cloud}|${tts.accent}|${text}`;
    let url = cache.current?.sig === sig ? cache.current.url : null;
    if (!url) {
      setPlay("loading");
      try {
        const buf = await api.speakCloud(text);
        if (my !== session.current) return;
        if (cache.current) URL.revokeObjectURL(cache.current.url);
        url = URL.createObjectURL(new Blob([buf], { type: "audio/wav" }));
        cache.current = { sig, url };
      } catch (e) {
        if (my !== session.current) return;
        setPlay("idle");
        setError(String(e));
        return;
      }
    }
    const a = new Audio(url);
    a.playbackRate = tts.rate;
    a.onended = () => my === session.current && setPlay("idle");
    audio.current = a;
    a.play().then(
      () => my === session.current && setPlay("playing"),
      () => my === session.current && (setPlay("idle"), setError("无法播放音频。")),
    );
  }, [text, tts, stop]);

  const pause = () => {
    if (tts?.engine === "system") window.speechSynthesis.pause();
    else audio.current?.pause();
    setPlay("paused");
  };
  const resume = () => {
    if (tts?.engine === "system") window.speechSynthesis.resume();
    else audio.current?.play();
    setPlay("playing");
  };

  // 打开朗读页 / 换了文字：自动读一遍。离开时停止。
  const started = useRef(-1);
  useEffect(() => {
    if (!tts || started.current === playKey) return;
    started.current = playKey;
    // 系统语音列表可能稍后才加载好。
    const synth = window.speechSynthesis;
    if (tts.engine === "system" && synth && synth.getVoices().length === 0) {
      const onVoices = () => start();
      synth.addEventListener("voiceschanged", onVoices, { once: true });
      return () => synth.removeEventListener("voiceschanged", onVoices);
    }
    start();
  }, [tts, playKey, start]);
  useEffect(() => {
    window.addEventListener("fluent:stop", stop);
    return () => {
      window.removeEventListener("fluent:stop", stop);
      stop();
    };
  }, [stop]);

  const update = (patch: Partial<Tts>) => {
    if (!settings) return;
    const next = { ...settings, tts: { ...settings.tts, ...patch } };
    setSettings(next);
    api.saveSettings(next).catch(() => {});
    // 语速对正在播放的云端音频立即生效；其他改动下次播放生效。
    if (patch.rate && audio.current) audio.current.playbackRate = patch.rate;
  };

  if (!tts) return null;
  const busy = play === "playing" || play === "paused" || play === "loading";

  return (
    <div className="speaker">
      <div className="row">
        {play === "playing" ? (
          <button className="primary" onClick={pause}>
            ⏸ 暂停
          </button>
        ) : play === "paused" ? (
          <button className="primary" onClick={resume}>
            ▶ 继续
          </button>
        ) : (
          <button className="primary" disabled={play === "loading"} onClick={start}>
            {play === "loading" ? "生成语音…" : "▶ 朗读"}
          </button>
        )}
        <button disabled={!busy} onClick={stop}>
          ■ 停止
        </button>
        <span className="spacer" />
        <span className="hint">{tts.engine === "system" ? "系统语音" : "云端语音"}</span>
      </div>
      <div className="row opts">
        <span className="label">语速</span>
        <div className="chips">
          {RATES.map((r) => (
            <button key={r} className={tts.rate === r ? "chip on" : "chip"} onClick={() => update({ rate: r })}>
              {r}×
            </button>
          ))}
        </div>
        <span className="label">口音</span>
        <div className="chips">
          {(["us", "uk"] as const).map((a) => (
            <button key={a} className={tts.accent === a ? "chip on" : "chip"} onClick={() => update({ accent: a })}>
              {a === "us" ? "美音" : "英音"}
            </button>
          ))}
        </div>
      </div>
      {voiceName && <p className="hint">语音：{voiceName}</p>}
      {error && (
        <div className="error">
          <p>{error}</p>
          {error.includes("设置") && <button onClick={() => api.openSettings()}>打开设置</button>}
        </div>
      )}
    </div>
  );
}
