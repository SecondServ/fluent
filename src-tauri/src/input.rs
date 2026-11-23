//! 模拟「复制 / 粘贴」快捷键，发给当前前台程序。只在用户点了按钮之后调用。

#[derive(Clone, Copy)]
pub enum Shortcut {
    Copy,
    Paste,
}

/// mac：⌘C / ⌘V（CGEvent，需要辅助功能权限）。
#[cfg(target_os = "macos")]
pub fn send(s: Shortcut) -> Result<(), String> {
    crate::mac_select::send_cmd(match s {
        Shortcut::Copy => core_graphics::event::KeyCode::ANSI_C,
        Shortcut::Paste => core_graphics::event::KeyCode::ANSI_V,
    })
}

/// Windows：Ctrl+C / Ctrl+V（SendInput，经 enigo）。
#[cfg(windows)]
pub fn send(s: Shortcut) -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let ch = match s {
        Shortcut::Copy => 'c',
        Shortcut::Paste => 'v',
    };
    let mut e = Enigo::new(&Settings::default()).map_err(|e| format!("无法模拟按键：{e}"))?;
    let err = |e: enigo::InputError| format!("模拟按键失败：{e}");
    e.key(Key::Control, Direction::Press).map_err(err)?;
    e.key(Key::Unicode(ch), Direction::Click).map_err(err)?;
    e.key(Key::Control, Direction::Release).map_err(err)?;
    Ok(())
}
