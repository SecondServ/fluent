# Fluent（暂定名）— 跨平台划词英文助手 · PRD + BUILD SPEC

> 版本 v0.2 · 2026-11-21（v0.3：两个平台都用划词按钮；mac 右键「服务」保留为备用） · 平台：**macOS + Windows，一套代码（Tauri 2）**
> 前身：「担当 / Leadership」（`~/Projects/职场智慧`，保留不动）。本项目借用它的交互模型（划词 → 小面板 → AI → 复制）与硬约束，但**不共享代码**。

---

## 1. 一句话

在**任意程序**里选中一段英文（或中文草稿），光标正下方会冒出一个长条按钮（五个功能），点哪个就弹出小面板做哪个：**翻译 / 总结 / 改写成地道英文 / 帮我回复 / 朗读**。结果一键复制或替换原文；发不发由你决定。

**目标用户**：每天收大量英文邮件和消息、英语非母语、懒得逐字读的人（也就是你）。

---

## 2. 功能（5 个，按优先级）

| # | 功能 | 输入 | 输出 | 复制 | 替换原文 |
|---|------|------|------|:--:|:--:|
| F1 | **翻译** | 选中的英文 | 忠实的中文译文（中文输入时反向翻成英文） | ✅ | — |
| F2 | **总结** | 长邮件/消息 | 中文：一句话主旨 + 要点 + **需要我做什么（含截止时间）** | ✅ | — |
| F3 | **改写草稿** | 我写的英文（可以夹中文） | 正式、地道的英文；可选「显示改了什么」 | ✅ | ✅ |
| F4 | **帮我回复** | 收到的英文邮件/消息 + 我用中文写的回复意图 | 可直接发送的英文回复 | ✅ | ✅ |
| F5 | **朗读** | 选中的英文 | 读出来；可调语速、可选美音/英音 | — | — |

### 2.1 细节

- **F1 翻译**：自动判断语言：英文 → 中文，中文 → 英文。保留数字、日期、人名、链接。缩写和行话在译文后用括号简单解释（例如 EOD、OOO、circle back）。
- **F2 总结**：固定输出结构：
  ```
  一句话：……
  要点：• … • …
  要我做的：• …（截止：…）    ← 没有就写「无需行动」
  ```
- **F3 改写**：语气档 4 个：**正式**（默认）/ 中性 / 友好 / 简洁。不改原意，不添加我没说的内容。「显示改了什么」打开时，在结果下方用中文列出 2–4 条主要改动，**顺便学英语**；复制和替换只取正文。
- **F4 回复**：面板上方一行显示原文摘要，下面是输入框，写回复意图（中文即可，可留空）。立场预设：**同意 / 婉拒 / 需要更多信息 / 稍后跟进 / 感谢确认**。语气沿用 F3 的语气档。结果可以直接在面板里编辑。
- **F5 朗读**：
  - 默认用**系统自带语音**：免费、离线、秒出。可选**云端语音**（OpenAI / Gemini TTS），更自然但要付费，在设置里切换。
  - 控件：播放/暂停、语速（0.75× / 1× / 1.25×）、口音（美/英，取决于系统已装的语音）。
  - 选中单个词或短语时，额外显示音标。音标由 AI 给出，放在 M3 之后作为可选项。

---

## 3. 交互

- **触发（不用快捷键）**：
  - **两个平台：划词弹按钮**。全局鼠标钩子（mac CGEventTap / Windows WH_MOUSE_LL）判断「拖选 / 双击」，且按下鼠标时系统光标须是 I 形文字光标（mac 比对 currentSystemCursor 的热点和尺寸，Windows 比对 IDC_IBEAM 句柄），在**光标正下方**冒出长条按钮（230×24 逻辑像素，五段等宽：翻译 / 总结 / 改写 / 回复 / 朗读）：水平中点对齐松开鼠标时的光标，顶边在光标下方 12 逻辑像素。点哪段就取文字并打开对应功能（后端按点击位置算段号）。5 秒自动消失，鼠标停在上面时不消失；点别处 / 右键 / 滚轮也消失。点按钮才模拟复制取文字（并还原剪贴板）。钩子只看鼠标坐标和光标形状，不读文字、不监听键盘。判断逻辑在 `bubble.rs`，两平台共用。
  - 面板同样弹在光标正下方、水平居中；超出屏幕往里收，下方放不下就放到光标上方。
  - **mac 需要「辅助功能」权限**（拦截按钮上的点击、模拟 ⌘C）。没授权时可用备用入口：右键 → 服务 → 「用 Fluent 翻译」（系统直接传文字，零权限）。
- **面板**：置顶、无边框、尽量不抢焦点；按 Esc 关闭。结果**流式输出**，边生成边显示。
- **常驻**：mac 在菜单栏，Windows 在系统托盘。托盘菜单：设置 / 退出。
- **快捷方式**：面板里数字键 1–5 对应五个功能；`⌘/Ctrl+C` 复制结果；`⌘/Ctrl+Enter` 替换原文。

---

## 4. 硬约束（沿用「担当」，不可违反）

1. **人在环中**：绝不自动发送任何消息。只做「复制」和用户点击后的「粘贴替换」。
2. **不做后台采集**：只在用户点了划词按钮（或 mac 右键菜单项）的那一刻读取选中文本。鼠标钩子只判断选择手势，不读文字、不记录。不抓取、不存档。
3. **日志安全**：日志里绝不出现 API Key 或用户文本。
4. **密钥存储**：存在系统凭据库（mac Keychain / Windows Credential Manager），不写明文配置。
5. **隐私提示**：设置页和 README 提示「公司内容发给第三方 AI 前请先确认公司政策；Gemini 请用付费 Key，免费 AI Studio Key 的数据可能被用于训练」。
6. **范围纪律**：按里程碑推进，每次只做当前里程碑。

---

## 5. 技术方案

```
┌──────────── 前端（WebView：React + TS + Vite）────────────┐
│ 功能面板 · 结果流式渲染 · 设置页 · 朗读（系统语音用 Web Speech API）│
└──────────────▲───────────────────────── invoke / event ───┘
               │
┌──────────────┴──────────── 后端（Rust，Tauri 2）────────────┐
│ mac      : CGEventTap 划词检测 + 不激活的按钮；NSServices 备用入口   │
│ win      : WH_MOUSE_LL 划词检测 + 不抢焦点的小按钮 (windows-sys)    │
│ grab(win): 模拟 Ctrl+C → 读剪贴板 → 还原原剪贴板 (enigo + arboard)  │
│ replace  : 写剪贴板 → 模拟 ⌘V/Ctrl+V → 还原                   │
│ ai       : Gemini / OpenAI / Claude 三家，reqwest 流式 (SSE)   │
│ prompts  : 5 个功能的模板（纯逻辑，单元测试覆盖）                  │
│ secrets  : keyring crate (Keychain / Credential Manager)   │
│ tts-cloud: 调云端 TTS 拿音频字节 → 交给前端播放                  │
│ tray     : 菜单栏 / 托盘                                     │
└────────────────────────────────────────────────────────────┘
```

**设计取舍**
- **API 调用放在 Rust 端**：Key 不进 WebView，前端只收流式文本。
- **取选中文字**：两平台都用「模拟复制 + 还原剪贴板」，覆盖面最广；mac AX API / Windows UI Automation 作为后续优化。mac 右键服务由系统直接传入。
- **系统朗读用 Web Speech API**（`speechSynthesis`）：mac 的 WKWebView 和 Windows 的 WebView2 都支持，直接调用系统语音，**零原生代码**。如果某个平台不好用，再换成 Rust `tts` crate。
- **权限**：mac 需要「辅助功能」（首次启动弹系统提示，设置页显示状态）。Windows 的鼠标钩子不需要授权（个别杀毒软件可能拦截）。
- **按钮不抢焦点**：mac 用 `orderFrontRegardless` 显示、Windows 设 `WS_EX_NOACTIVATE`；点击由钩子直接吞掉，原程序保持前台和选区，随后的 Ctrl+C 才会发到原程序。
- **焦点问题（最大技术风险）**：面板弹出时不能让原 App 丢失选区或焦点，否则「替换原文」会粘错地方。M0 里就先验证；替换前记住原前台窗口，粘贴前把它切回来。

**AI 模型**：沿用「担当」的多厂商结构：Gemini（推荐、付费 Key）/ OpenAI / Claude，设置里选一家并填 Key。所有功能共用一个快模型（如 gemini-2.5-flash，关闭思考），速度优先。

---

## 6. Prompt 规格

> 通用 system：`You are a writing assistant for a non-native English speaker working in an international company. Output only the result itself — no preamble, no explanations, no quotes. The result must be ready to copy and use.`

### F1 翻译
```
Translate the text below.
- If it is mainly English, translate into Simplified Chinese; if mainly Chinese, translate into natural English.
- Be faithful and complete: keep every number, date, name, link and requirement.
- For workplace jargon or acronyms (e.g. EOD, OOO, circle back), add a short Chinese explanation in parentheses the first time they appear.
Text:
{{TEXT}}
```

### F2 总结
```
Summarize the message below in Simplified Chinese using EXACTLY this format:
一句话：<one sentence: what this message is about>
要点：
• <key point>（最多 5 条，只保留真正重要的）
要我做的：
• <action requested from the reader, with deadline if any>   （若没有，写「无需行动」）
Do not invent anything not in the message.
Message:
{{TEXT}}
```

### F3 改写
```
Rewrite my draft into clear, natural, native-sounding English for a workplace message.
- Tone: {{TONE}}   (formal | neutral | friendly | concise)
- The draft may contain Chinese or broken English — understand my intent and express it in English.
- Keep my meaning; do not add facts, promises or details I did not write.
- Keep the same format (email stays an email, chat stays short).
{{#if EXPLAIN}}After the rewrite, output a line "---" and then 2–4 bullets in Simplified Chinese explaining the most useful changes (word choice, grammar, tone).{{/if}}
Draft:
{{TEXT}}
```

### F4 回复
```
Write an English reply to the message below.
- My intent (may be in Chinese, may be empty): {{INTENT}}
- Stance: {{STANCE}}   (agree | decline politely | ask for more info | will follow up later | thank and confirm)
- Tone: {{TONE}}
- Match the format of the original (email → email with greeting/sign-off; chat → short message). Do not invent facts, dates or commitments beyond my intent.
Output only the reply body.
Message:
{{TEXT}}
```

### F5 音标（可选，M3 之后）
```
Give the IPA (US and UK) for the word or short phrase below, plus one tip in Simplified Chinese on how to pronounce it. Format: "US /…/  UK /…/\n<tip>"
{{TEXT}}
```

---

## 7. 里程碑（每步都能独立验证）

**M0 — 打通链路（两端）**
- Tauri 2 骨架 + 托盘 + 设置页（选厂商、填 Key → 系统凭据库）。
- 划词 → 光标下方小按钮 → 模拟复制取文字（还原剪贴板）→ 光标下方弹面板 → **F1 翻译**流式出结果 → 复制。mac 另有右键「服务」备用入口。
- GitHub Actions：在 mac 和 Windows 上各打一次包（本机只有 mac，Windows 靠 CI 构建，再到 Windows 机器上手测）。
- ✅ 验收：在 Safari / Chrome / 邮件 / Outlook / Slack 里选中英文，通过划词按钮出中文译文，能复制；Windows 上剪贴板原内容被还原。记录哪些程序里按钮不出现或取不到文字。

**M1 — 读类功能** ✅：F2 总结；面板功能切换（数字键）；错误、限流、无 Key 时的提示和重试。

**M2 — 写类功能** ✅：F3 改写（语气档 + 显示改动）、F4 回复（意图输入 + 立场 + 可编辑结果）、「替换原文」（切回原窗口再粘贴）。

**M3 — 朗读** ✅：F5 系统语音（Web Speech，语速、口音）+ 可选云端 TTS（OpenAI `audio/speech` / Gemini TTS，返回 WAV，同段文字缓存不重复收费）；单词 / 短语附 AI 音标。

**M4 — 打磨（可选）**：开机自启、安装包（mac `.dmg` / Windows `.msi`）与签名说明、README。

---

## 8. 验证清单

- [ ] 点菜单项 / 按钮到面板出现 < 300ms；首字流式出现 < 1.5s。
- [ ] Outlook、Chrome、Slack、Word、微信/飞书都能取到选中文字（mac 和 Windows 都测）。
- [ ] （Windows）取文字后原剪贴板被还原；什么都没选时提示「没选中文字」，不会把旧剪贴板内容发出去。
- [ ] 「替换原文」粘贴到正确的窗口和位置。
- [ ] Key 只存在系统凭据库；日志里没有 Key 和用户文本。
- [ ] 无网络、401、429 都有友好提示和重试。

---

## 9. 待定 / 开放问题

- **项目名**：Fluent 是暂定名，随时可改。
- **mac 签名**：沿用「担当」的自签证书（固定签名，钥匙串的「始终允许」在重新编译后不会失效）。
- **Windows 安装包签名**：没有代码签名证书时，SmartScreen 会提示「未知发布者」。分发给别人前再决定要不要买证书。
