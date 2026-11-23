import React from "react";
import ReactDOM from "react-dom/client";
import Bubble from "./Bubble";
import Panel from "./Panel";
import SettingsPage from "./Settings";
import "./styles.css";

// 开发时在普通浏览器里打开（没有 Tauri 后端）→ 加载假后端，方便调界面。
if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  await import("./devMock");
}

// 同一份前端，按窗口区分：index.html#panel / #settings / #bubble（划词长条按钮）
const view = window.location.hash.replace("#", "");
document.body.dataset.view = view;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {view === "settings" ? <SettingsPage /> : view === "bubble" ? <Bubble /> : <Panel />}
  </React.StrictMode>,
);
