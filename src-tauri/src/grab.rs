//! 取选中文字：放一个哨兵到剪贴板 → 模拟复制 → 等剪贴板变化 → 读出 → 还原原剪贴板。
//!
//! 只在用户点了划词按钮的那一刻调用（硬约束：不做后台采集）。不记录任何文本。

use arboard::{Clipboard, ImageData};
use std::thread::sleep;
use std::time::{Duration, Instant};

enum Saved {
    Text(String),
    Image(ImageData<'static>),
    Nothing,
}

fn save(cb: &mut Clipboard) -> Saved {
    if let Ok(t) = cb.get_text() {
        return Saved::Text(t);
    }
    if let Ok(img) = cb.get_image() {
        return Saved::Image(img.to_owned_img());
    }
    Saved::Nothing
}

fn restore(cb: &mut Clipboard, saved: Saved) {
    let _ = match saved {
        Saved::Text(t) => cb.set_text(t),
        Saved::Image(img) => cb.set_image(img),
        Saved::Nothing => cb.clear(),
    };
}

/// 把 `text` 粘贴到当前前台程序（替换选区），之后还原原剪贴板。
pub fn paste_text(text: &str) -> Result<(), String> {
    let mut cb = Clipboard::new().map_err(|e| format!("无法访问剪贴板：{e}"))?;
    let saved = save(&mut cb);
    cb.set_text(text.to_string()).map_err(|e| format!("无法写入剪贴板：{e}"))?;
    sleep(Duration::from_millis(50));
    let sent = crate::input::send(crate::input::Shortcut::Paste);
    // 等目标程序读完剪贴板再还原。
    sleep(Duration::from_millis(400));
    restore(&mut cb, saved);
    sent
}

/// 返回 Ok(None) 表示没有选中文字。
pub fn grab_selection() -> Result<Option<String>, String> {
    let mut cb = Clipboard::new().map_err(|e| format!("无法访问剪贴板：{e}"))?;
    let saved = save(&mut cb);

    let sentinel = format!("\u{2063}fluent-grab-{}", std::process::id());
    cb.set_text(sentinel.clone())
        .map_err(|e| format!("无法写入剪贴板：{e}"))?;

    let sent = crate::input::send(crate::input::Shortcut::Copy);

    let mut grabbed = None;
    if sent.is_ok() {
        let deadline = Instant::now() + Duration::from_millis(600);
        while Instant::now() < deadline {
            sleep(Duration::from_millis(25));
            match cb.get_text() {
                Ok(t) if t != sentinel => {
                    grabbed = Some(t);
                    break;
                }
                Ok(_) => {}
                // 复制的是非文本内容（如图片），哨兵已被替换但读不出文字。
                Err(_) => break,
            }
        }
    }

    restore(&mut cb, saved);
    sent?;

    Ok(grabbed.filter(|t| !t.trim().is_empty()))
}
