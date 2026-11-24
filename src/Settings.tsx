import { useEffect, useState } from "react";
import { api, isMac, Provider, PROVIDERS, Settings, Tts } from "./api";

/** API Key 输入框：Key 存进系统凭据库，不回显。 */
function KeyField({ provider, label, flash }: { provider: Provider; label: string; flash: (m: string) => void }) {
  const [hasKey, setHasKey] = useState(false);
  const [keyInput, setKeyInput] = useState("");

  useEffect(() => {
    api.hasApiKey(provider).then(setHasKey);
    setKeyInput("");
  }, [provider]);

  const saveKey = async () => {
    try {
      await api.setApiKey(provider, keyInput);
      setKeyInput("");
      const ok = await api.hasApiKey(provider);
      setHasKey(ok);
      flash(ok ? "Key 已保存到系统凭据库" : "Key 已清除");
    } catch (e) {
      flash(String(e));
    }
  };

  return (
    <label className="field">
      <span>
        {label} {hasKey ? <em className="ok">✓ 已保存</em> : <em className="warn">未填写</em>}
      </span>
      <div className="row">
        <input
          type="password"
          placeholder={hasKey ? "已保存（输入新 Key 可替换，留空保存可清除）" : "粘贴你的 API Key"}
          value={keyInput}
          onChange={(e) => setKeyInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && saveKey()}
        />
        <button onClick={saveKey}>保存</button>
      </div>
    </label>
  );
}

export default function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [permitted, setPermitted] = useState(true);

  // mac：辅助功能权限状态。在系统设置里打开后这里会自动变成 ✓。
  useEffect(() => {
    if (!isMac) return;
    const check = () => api.triggerPermission().then(setPermitted);
    check();
    const t = setInterval(check, 2000);
    return () => clearInterval(t);
  }, []);

  useEffect(() => {
    api.getSettings().then(setSettings);
  }, []);

  if (!settings) return null;
  const provider = settings.provider;
  const models = settings[provider];

  const flash = (m: string) => {
    setMsg(m);
    setTimeout(() => setMsg(null), 2000);
  };

  const update = (next: Settings) => {
    setSettings(next);
    api.saveSettings(next).catch((e) => flash(`保存失败：${e}`));
  };

  const tts = settings.tts;
  const updateTts = (patch: Partial<Tts>) => update({ ...settings, tts: { ...tts, ...patch } });

  return (
    <div className="settings">
      <h1>Fluent 设置</h1>
      <section>
        <h2>怎么用</h2>
        <p>
          在任意程序里选中文字（在文字上拖选或双击），光标正下方会冒出一个长条：翻译 | 总结 | 改写 | 回复 | 朗读，点哪个就做哪个。
        </p>
        <p className="note">
          只有按下鼠标时光标是文字光标（I 形）才会出现，在桌面、标题栏、图片上拖动不会。长条几秒后自动消失（鼠标停在上面时不会），点别处也会消失。Fluent 只看鼠标位置和光标形状，点了按钮才读取选中的内容。
        </p>
        {isMac && (
          <>
            <p className="field">
              <span>
                辅助功能权限{" "}
                {permitted ? <em className="ok">✓ 已开启</em> : <em className="warn">未开启，长条不会出现</em>}
              </span>
            </p>
            {!permitted && (
              <div className="row">
                <button onClick={() => api.openPermissionSettings()}>打开系统设置</button>
              </div>
            )}
            <p className="note">
              需要在 系统设置 → 隐私与安全性 → 辅助功能 里打开 Fluent，用来感知鼠标选字、在你点按钮后模拟 ⌘C
              复制选中的文字。没开权限时，也可以选中文字 → 右键 → 服务 → 用 Fluent 翻译。
            </p>
          </>
        )}
      </section>

      <section>
        <h2>AI 厂商</h2>
        <div className="providers">
          {PROVIDERS.map((p) => (
            <label key={p.id} className={provider === p.id ? "sel" : ""}>
              <input
                type="radio"
                name="provider"
                checked={provider === p.id}
                onChange={() => update({ ...settings, provider: p.id as Provider })}
              />
              {p.name}
            </label>
          ))}
        </div>
        <p className="note">{PROVIDERS.find((p) => p.id === provider)?.hint}</p>

        <KeyField provider={provider} label="API Key" flash={flash} />

        {provider === "openai" && (
          <label className="field">
            <span>Base URL</span>
            <input
              value={settings.openai_base_url}
              onChange={(e) => update({ ...settings, openai_base_url: e.target.value })}
            />
          </label>
        )}

        <label className="field">
          <span>模型（所有功能共用，选快的）</span>
          <input
            value={models.model}
            onChange={(e) => update({ ...settings, [provider]: { model: e.target.value } })}
          />
        </label>
      </section>

      <section>
        <h2>朗读</h2>
        <div className="providers">
          <label className={tts.engine === "system" ? "sel" : ""}>
            <input type="radio" checked={tts.engine === "system"} onChange={() => updateTts({ engine: "system" })} />
            系统语音（免费、离线）
          </label>
          <label className={tts.engine === "cloud" ? "sel" : ""}>
            <input type="radio" checked={tts.engine === "cloud"} onChange={() => updateTts({ engine: "cloud" })} />
            云端语音（更自然，按量付费）
          </label>
        </div>
        {tts.engine === "system" ? (
          <p className="note">
            用系统自带的英文语音。想要更好听的：
            {isMac
              ? "系统设置 → 辅助功能 → 朗读内容 → 系统声音 → 管理声音，下载英文的「增强」或「高级」语音。"
              : "设置 → 时间和语言 → 语音，添加英语（美国 / 英国）语音包。"}
          </p>
        ) : (
          <>
            <div className="providers" style={{ marginTop: 10 }}>
              {(["openai", "gemini"] as const).map((p) => (
                <label key={p} className={tts.cloud === p ? "sel" : ""}>
                  <input type="radio" checked={tts.cloud === p} onChange={() => updateTts({ cloud: p })} />
                  {p === "openai" ? "OpenAI" : "Google Gemini"}
                </label>
              ))}
            </div>
            <KeyField
              provider={tts.cloud}
              label={`${tts.cloud === "openai" ? "OpenAI" : "Gemini"} API Key（和上面同一家时共用）`}
              flash={flash}
            />
            <div className="grid2">
              <label className="field">
                <span>语音模型</span>
                <input
                  value={tts.cloud === "openai" ? tts.openai_model : tts.gemini_model}
                  onChange={(e) =>
                    updateTts(tts.cloud === "openai" ? { openai_model: e.target.value } : { gemini_model: e.target.value })
                  }
                />
              </label>
              <label className="field">
                <span>声音</span>
                <input
                  value={tts.cloud === "openai" ? tts.openai_voice : tts.gemini_voice}
                  onChange={(e) =>
                    updateTts(tts.cloud === "openai" ? { openai_voice: e.target.value } : { gemini_voice: e.target.value })
                  }
                />
              </label>
            </div>
            <p className="note">
              {tts.cloud === "openai"
                ? "OpenAI 用上面「OpenAI 兼容」的 Base URL。声音可选 alloy、ash、coral、echo、nova、sage、shimmer 等。"
                : "声音可选 Kore、Puck、Charon、Aoede、Fenrir 等。"}
              同一段文字重复播放不会重复收费。
            </p>
          </>
        )}
      </section>

      <section>
        <h2>隐私</h2>
        <ul className="note">
          <li>
            只在你点了划词按钮（或右键菜单里的「用 Fluent 翻译」）之后才读取选中的文字，不保存任何内容。
          </li>
          <li>API Key 存在系统凭据库（{isMac ? "钥匙串" : "Windows 凭据管理器"}），不写入任何文件。</li>
          <li>「替换原文」只会切回原程序粘贴结果，不会按回车、不会替你发送。</li>
          <li>公司内容发给第三方 AI 之前，请先确认公司政策。</li>
        </ul>
      </section>

      {msg && <div className="toast">{msg}</div>}
    </div>
  );
}
