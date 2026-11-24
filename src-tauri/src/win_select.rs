//! Windows：划词弹按钮。
//!
//! 用低级鼠标钩子（WH_MOUSE_LL）监听全局鼠标，判断逻辑在 [`crate::bubble`]。
//! 点按钮的那一下由钩子直接吞掉（返回 1），按钮窗口也设成 WS_EX_NOACTIVATE，
//! 所以原程序始终保持前台和选区，随后模拟的 Ctrl+C 才会发到原程序。

use crate::bubble::{self, Backend, Config, Mouse, Pt, Rect};
use std::ptr::null_mut;
use std::time::Duration;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONEAREST};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

struct Win {
    /// 按钮窗口的 HWND（存成 isize 以便跨线程）。
    bubble: isize,
    on_click: Box<dyn Fn(usize) + Send + Sync>,
}

/// 光标所在显示器的缩放比例（物理像素 / 逻辑像素）。
fn scale_at(p: Pt) -> f64 {
    unsafe {
        let mon = MonitorFromPoint(POINT { x: p.x as i32, y: p.y as i32 }, MONITOR_DEFAULTTONEAREST);
        let (mut dx, mut dy) = (96u32, 96u32);
        GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        dx.max(96) as f64 / 96.0
    }
}

impl Backend for Win {
    fn show(&self, cursor: Pt) -> Rect {
        let scale = scale_at(cursor);
        let (w, h) = ((bubble::WIDTH * scale).round(), (bubble::HEIGHT * scale).round());
        let r = bubble::below_cursor(cursor, w, h, scale);
        unsafe {
            let hwnd = self.bubble as HWND;
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                r.x.round() as i32,
                r.y.round() as i32,
                w as i32,
                h as i32,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        r
    }

    fn hide(&self) {
        unsafe {
            ShowWindow(self.bubble as HWND, SW_HIDE);
        }
    }

    fn foreground_is_ours(&self) -> bool {
        unsafe {
            let fg = GetForegroundWindow();
            if fg.is_null() {
                return false;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(fg, &mut pid);
            pid == GetCurrentProcessId()
        }
    }

    fn clicked(&self, index: usize) {
        (self.on_click)(index);
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let p = unsafe { (*(lparam as *const MSLLHOOKSTRUCT)).pt };
        let pt = Pt { x: p.x as f64, y: p.y as f64 };
        let ev = match wparam as u32 {
            WM_LBUTTONDOWN => Some(Mouse::LeftDown(pt)),
            WM_LBUTTONUP => Some(Mouse::LeftUp { pt, clicks: None }),
            WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_MOUSEWHEEL => Some(Mouse::Other),
            WM_MOUSEMOVE => Some(Mouse::Move(pt)),
            _ => None,
        };
        if ev.is_some_and(bubble::on_mouse) {
            return 1;
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

/// 启动划词检测。`bubble` 是按钮窗口的 HWND；`on_click(第几段)` 在用户点按钮后（后台线程）调用。
pub fn start(bubble_hwnd: HWND, on_click: impl Fn(usize) + Send + Sync + 'static) {
    let scale = unsafe { GetDpiForWindow(bubble_hwnd) }.max(96) as f64 / 96.0;
    let double_click = Duration::from_millis(unsafe { GetDoubleClickTime() } as u64);
    bubble::install(
        Win { bubble: bubble_hwnd as isize, on_click: Box::new(on_click) },
        Config { scale, double_click },
    );
    unsafe {
        // 按钮窗口：不抢焦点、不进任务栏。
        let ex = GetWindowLongPtrW(bubble_hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            bubble_hwnd,
            GWL_EXSTYLE,
            ex | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST) as isize,
        );
    }
    std::thread::spawn(|| unsafe {
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), null_mut(), 0);
        if hook.is_null() {
            eprintln!("win_select: SetWindowsHookExW failed");
            return;
        }
        // 低级钩子需要本线程有消息循环。
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        UnhookWindowsHookEx(hook);
    });
}
