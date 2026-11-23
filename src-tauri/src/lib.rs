mod ai;
mod bubble;
#[cfg(any(target_os = "macos", windows))]
mod grab;
#[cfg(any(target_os = "macos", windows))]
mod input;
#[cfg(target_os = "macos")]
mod mac_select;
#[cfg(target_os = "macos")]
mod mac_services;
mod prompts;
mod settings;
mod source_app;
mod tts;
#[cfg(windows)]
mod win_select;

use serde::Serialize;
use settings::{Provider, Settings};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::ipc::Channel;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow, WindowEvent};

const PANEL: &str = "panel";
const SETTINGS: &str = "settings";
const BUBBLE: &str = "bubble";

/// 我们自己的窗口（面板 / 设置）是否在前台：是就不冒划词按钮。
static OURS_FOCUSED: AtomicBool = AtomicBool::new(false);

struct AppState {
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    /// 最近一次抓取结果（面板首次加载时取用）。
    last_grab: Mutex<Option<GrabResult>>,
    /// 面板最近一次弹出的时间：刚弹出时焦点可能还在原程序手里，这段时间的失焦不收起面板。
    panel_shown_at: Mutex<Option<std::time::Instant>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GrabResult {
    /// 每次触发自增，前端用来丢弃过期的流。
    id: u64,
    text: Option<String>,
    error: Option<String>,
    /// 面板要打开的功能页（点了长条上的哪个按钮）；None = 保持上次的页。
    tab: Option<&'static str>,
}

/// 长条按钮上的功能，顺序和前端 Bubble.tsx 一致。
const TABS: [&str; bubble::SEGMENTS] = ["translate", "summarize", "rewrite", "reply", "read"];

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum StreamEvent {
    Delta { text: String },
    Done,
    Error { message: String },
}

// ---------- commands ----------

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: tauri::State<AppState>, settings: Settings) -> Result<(), String> {
    settings.save(&state.settings_path)?;
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

// 读写钥匙串可能弹系统确认框而阻塞，所以用 async 命令（不在主线程上跑）。
#[tauri::command]
async fn has_api_key(provider: Provider) -> bool {
    settings::get_key(provider).is_some()
}

#[tauri::command]
async fn set_api_key(provider: Provider, key: String) -> Result<(), String> {
    settings::set_key(provider, &key)
}

#[tauri::command]
fn last_grab(state: tauri::State<AppState>) -> Option<GrabResult> {
    state.last_grab.lock().unwrap().clone()
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut cb| cb.set_text(text))
        .map_err(|e| format!("复制失败：{e}"))
}

#[tauri::command]
fn hide_panel(app: AppHandle) {
    if let Some(w) = app.get_webview_window(PANEL) {
        let _ = w.hide();
    }
}

#[tauri::command]
fn open_settings(app: AppHandle) {
    show_settings(&app);
}

/// 划词按钮需要的系统权限是否已开（mac：辅助功能；其他平台不需要）。
#[tauri::command]
fn trigger_permission() -> bool {
    #[cfg(target_os = "macos")]
    return mac_select::trusted();
    #[cfg(not(target_os = "macos"))]
    true
}

#[tauri::command]
fn open_permission_settings() {
    #[cfg(target_os = "macos")]
    mac_select::open_settings();
}

#[tauri::command]
async fn run_feature(
    state: tauri::State<'_, AppState>,
    req: prompts::Request,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    let settings = state.settings.lock().unwrap().clone();
    let prompt = prompts::build(&req);
    let result = ai::stream(&settings, &prompt, |t| {
        let _ = on_event.send(StreamEvent::Delta { text: t });
    })
    .await;
    let _ = on_event.send(match result {
        Ok(()) => StreamEvent::Done,
        Err(message) => StreamEvent::Error { message },
    });
    Ok(())
}

/// 云端朗读：返回 WAV 音频（二进制，不走 JSON）。
#[tauri::command]
async fn speak_cloud(state: tauri::State<'_, AppState>, text: String) -> Result<tauri::ipc::Response, String> {
    let settings = state.settings.lock().unwrap().clone();
    tts::speak(&settings, &text).await.map(tauri::ipc::Response::new)
}

/// 替换原文：收起面板 → 切回取文字时的程序 → 粘贴（原选区被替换），之后还原剪贴板。
/// 只在用户点了「替换原文」后调用；不会按回车、不会发送。
#[tauri::command]
async fn replace_text(app: AppHandle, text: String) -> Result<(), String> {
    #[cfg(any(target_os = "macos", windows))]
    {
        if let Some(w) = app.get_webview_window(PANEL) {
            let _ = w.hide();
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = app.run_on_main_thread(move || {
            let _ = tx.send(source_app::activate());
        });
        if !rx.recv().unwrap_or(false) {
            if let Some(w) = app.get_webview_window(PANEL) {
                let _ = w.show();
                let _ = w.set_focus();
            }
            return Err("找不到原来的窗口，请改用「复制」后手动粘贴。".into());
        }
        // 等原程序回到前台、恢复选区。
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        grab::paste_text(&text)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (app, text);
        Err("当前系统不支持替换原文。".into())
    }
}

// ---------- windows ----------

fn show_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SETTINGS) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// 把面板放在光标正下方、水平中点对齐光标；超出屏幕时往里收，下面放不下就放到光标上方。
fn place_panel_below_cursor(w: &WebviewWindow) {
    let Ok(cursor) = w.cursor_position() else { return };
    let Ok(size) = w.outer_size() else { return };
    let scale = w.scale_factor().unwrap_or(1.0);
    let (pw, ph) = (size.width as f64, size.height as f64);
    let r = bubble::below_cursor(bubble::Pt { x: cursor.x, y: cursor.y }, pw, ph, scale);
    let (mut x, mut y) = (r.x, r.y);
    if let Ok(Some(m)) = w.monitor_from_point(cursor.x, cursor.y) {
        let (mx, my) = (m.position().x as f64, m.position().y as f64);
        let (mw, mh) = (m.size().width as f64, m.size().height as f64);
        x = x.clamp(mx, (mx + mw - pw).max(mx));
        if y + ph > my + mh {
            y = (cursor.y - bubble::GAP * scale - ph).max(my);
        }
    }
    let _ = w.set_position(PhysicalPosition::new(x, y));
}

/// 把取到的文字（或错误）交给面板：放到鼠标旁边、显示、通知前端。
fn deliver(app: &AppHandle, text: Option<String>, error: Option<String>, tab: Option<&'static str>) {
    // 记住原程序（替换原文时切回去）；此刻面板还没弹出，前台就是原程序。
    source_app::remember();
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let text = text.filter(|t| !t.trim().is_empty());
    let error = match (&text, error) {
        (Some(_), _) => None,
        (None, Some(e)) => Some(e),
        (None, None) => Some("没有读到选中的文字。先选中一段文字再试。".into()),
    };
    let result = GrabResult { id, text, error, tab };
    let state = app.state::<AppState>();
    *state.last_grab.lock().unwrap() = Some(result.clone());
    *state.panel_shown_at.lock().unwrap() = Some(std::time::Instant::now());
    if let Some(w) = app.get_webview_window(PANEL) {
        place_panel_below_cursor(&w);
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit_to(PANEL, "grab", result);
    }
}

/// 划词按钮窗口（mac / Windows 共用）：透明、无边框、置顶、不进任务栏，平时隐藏。
#[cfg(any(target_os = "macos", windows))]
fn build_bubble(app: &tauri::App) -> tauri::Result<WebviewWindow> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    WebviewWindowBuilder::new(app, BUBBLE, WebviewUrl::App("index.html#bubble".into()))
        .title("Fluent")
        .inner_size(bubble::WIDTH, bubble::HEIGHT)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .build()
}

/// 点了划词按钮：模拟复制取出选中文字，弹面板。
#[cfg(any(target_os = "macos", windows))]
fn grab_and_deliver(app: &AppHandle, index: usize) {
    let tab = TABS.get(index).copied();
    match grab::grab_selection() {
        Ok(text) => deliver(app, text, None, tab),
        Err(e) => deliver(app, None, Some(e), tab),
    }
}

#[cfg(target_os = "macos")]
struct MacBubble {
    app: AppHandle,
}

#[cfg(target_os = "macos")]
impl bubble::Backend for MacBubble {
    fn show(&self, cursor: bubble::Pt) -> bubble::Rect {
        // mac 的鼠标坐标就是逻辑点，原点在主屏左上角，和 Tauri 的逻辑坐标一致。
        let r = bubble::below_cursor(cursor, bubble::WIDTH, bubble::HEIGHT, 1.0);
        let app = self.app.clone();
        let _ = self.app.run_on_main_thread(move || {
            let Some(w) = app.get_webview_window(BUBBLE) else { return };
            let _ = w.set_position(tauri::LogicalPosition::new(r.x, r.y));
            // 用 orderFrontRegardless 显示：不激活 Fluent、不抢原程序的焦点。
            if let Ok(ns) = w.ns_window() {
                unsafe { (*(ns as *const objc2_app_kit::NSWindow)).orderFrontRegardless() };
            }
        });
        r
    }

    fn hide(&self) {
        if let Some(w) = self.app.get_webview_window(BUBBLE) {
            let _ = w.hide();
        }
    }

    fn foreground_is_ours(&self) -> bool {
        OURS_FOCUSED.load(Ordering::Relaxed)
    }

    fn clicked(&self, index: usize) {
        grab_and_deliver(&self.app, index);
    }
}

/// 让窗口在所有桌面空间、包括全屏程序上方都能出现。
#[cfg(target_os = "macos")]
fn float_everywhere(w: &WebviewWindow, level: isize) {
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior as B};
    if let Ok(ns) = w.ns_window() {
        let ns = unsafe { &*(ns as *const NSWindow) };
        ns.setLevel(level);
        ns.setCollectionBehavior(B::CanJoinAllSpaces | B::FullScreenAuxiliary | B::Stationary | B::IgnoresCycle);
    }
}

/// mac：选中文字后光标下方冒出按钮（需要辅助功能权限）；右键菜单「服务」→ 用 Fluent 翻译 也保留。
#[cfg(target_os = "macos")]
fn setup_trigger(app: &tauri::App) -> tauri::Result<()> {
    let bubble_win = build_bubble(app)?;
    float_everywhere(&bubble_win, 25); // NSStatusWindowLevel
    if let Some(panel) = app.get_webview_window(PANEL) {
        float_everywhere(&panel, 3); // NSFloatingWindowLevel
    }
    bubble::install(
        MacBubble { app: app.handle().clone() },
        bubble::Config { scale: 1.0, double_click: std::time::Duration::from_millis(500) },
    );
    mac_select::start();

    let handle = app.handle().clone();
    mac_services::register(move |text| {
        // 系统在服务调用返回后才把焦点还给原程序；稍等再弹面板，免得面板一出来就失焦被收起。
        let handle = handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            deliver(&handle, Some(text), None, Some("translate"));
        });
    });
    Ok(())
}

/// Windows：选中文字后光标下方冒出按钮，点它再取文字。
#[cfg(windows)]
fn setup_trigger(app: &tauri::App) -> tauri::Result<()> {
    let bubble_win = build_bubble(app)?;
    let hwnd = bubble_win.hwnd()?.0 as windows_sys::Win32::Foundation::HWND;
    let handle = app.handle().clone();
    win_select::start(hwnd, move |i| grab_and_deliver(&handle, i));
    Ok(())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn setup_trigger(_app: &tauri::App) -> tauri::Result<()> {
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let settings = Settings::load(&settings_path);
            let provider = settings.provider;
            app.manage(AppState {
                settings: Mutex::new(settings),
                settings_path,
                last_grab: Mutex::new(None),
                panel_shown_at: Mutex::new(None),
            });

            // 托盘 / 菜单栏
            let open = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 Fluent", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            TrayIconBuilder::with_id("tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Fluent")
                .menu(&menu)
                .on_menu_event(|app, e| match e.id().as_ref() {
                    "settings" => show_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            setup_trigger(app)?;

            // 没填 Key 就打开设置。读钥匙串可能弹确认框，放到后台线程，别卡住主线程。
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                if settings::get_key(provider).is_none() {
                    show_settings(&handle);
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| match (window.label(), event) {
            // 设置窗口：关闭 = 隐藏（App 常驻托盘）。
            (SETTINGS, WindowEvent::CloseRequested { api, .. }) => {
                api.prevent_close();
                let _ = window.hide();
            }
            (PANEL | SETTINGS, WindowEvent::Focused(true)) => OURS_FOCUSED.store(true, Ordering::Relaxed),
            (SETTINGS, WindowEvent::Focused(false)) => OURS_FOCUSED.store(false, Ordering::Relaxed),
            // 面板：失去焦点就收起。
            (PANEL, WindowEvent::Focused(false)) => {
                OURS_FOCUSED.store(false, Ordering::Relaxed);
                let shown_at = *window.state::<AppState>().panel_shown_at.lock().unwrap();
                if shown_at.is_none_or(|t| t.elapsed() > std::time::Duration::from_millis(800)) {
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            has_api_key,
            set_api_key,
            last_grab,
            copy_text,
            hide_panel,
            open_settings,
            run_feature,
            trigger_permission,
            open_permission_settings,
            speak_cloud,
            replace_text,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
