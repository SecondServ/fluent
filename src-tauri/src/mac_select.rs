//! mac：划词弹按钮。
//!
//! 用 CGEventTap 监听全局鼠标（需要「辅助功能」权限），判断逻辑在 [`crate::bubble`]。
//! 点按钮的那一下在 tap 里直接丢掉，按钮窗口收不到点击，原程序始终保持前台和选区，
//! 随后模拟的 ⌘C 才会发到原程序。

use crate::bubble::{self, Mouse, Pt};
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::mach_port::CFMachPortRef;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::Duration;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
}

/// 有没有「辅助功能」权限。
pub fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// 弹系统的「Fluent 想控制这台电脑」提示（带「打开系统设置」按钮）。
fn prompt_trust() {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let opts = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
        AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef());
    }
}

/// 打开 系统设置 → 隐私与安全性 → 辅助功能。
pub fn open_settings() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn();
}

/// 模拟 ⌘+某键（⌘C / ⌘V），发给当前前台程序。
pub fn send_cmd(key: core_graphics::event::CGKeyCode) -> Result<(), String> {
    let err = || "模拟按键失败。请在 系统设置 → 隐私与安全性 → 辅助功能 里打开 Fluent。".to_string();
    let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).map_err(|_| err())?;
    for down in [true, false] {
        let e = CGEvent::new_keyboard_event(src.clone(), key, down).map_err(|_| err())?;
        e.set_flags(CGEventFlags::CGEventFlagCommand);
        e.post(CGEventTapLocation::HID);
    }
    Ok(())
}

static TAP_PORT: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

fn tap_callback(etype: CGEventType, event: &CGEvent) -> CallbackResult {
    let loc = event.location();
    let pt = Pt { x: loc.x, y: loc.y };
    let ev = match etype {
        CGEventType::LeftMouseDown => Mouse::LeftDown(pt),
        CGEventType::LeftMouseUp => Mouse::LeftUp {
            pt,
            clicks: Some(event.get_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE)),
        },
        CGEventType::RightMouseDown | CGEventType::OtherMouseDown | CGEventType::ScrollWheel => Mouse::Other,
        CGEventType::MouseMoved => Mouse::Move(pt),
        // 回调太慢或用户输入时系统会停掉 tap，重新打开。
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
            let port = TAP_PORT.load(Ordering::Relaxed);
            if !port.is_null() {
                unsafe { CGEventTapEnable(port as CFMachPortRef, true) };
            }
            return CallbackResult::Keep;
        }
        _ => return CallbackResult::Keep,
    };
    if bubble::on_mouse(ev) { CallbackResult::Drop } else { CallbackResult::Keep }
}

/// 启动划词检测（后台线程）。没有辅助功能权限时先弹系统提示，拿到权限后自动开始。
pub fn start() {
    std::thread::spawn(|| {
        if !trusted() {
            prompt_trust();
            while !trusted() {
                std::thread::sleep(Duration::from_millis(1500));
            }
        }
        let tap = loop {
            match CGEventTap::new(
                CGEventTapLocation::Session,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                vec![
                    CGEventType::LeftMouseDown,
                    CGEventType::LeftMouseUp,
                    CGEventType::RightMouseDown,
                    CGEventType::OtherMouseDown,
                    CGEventType::ScrollWheel,
                    CGEventType::MouseMoved,
                ],
                |_proxy, etype, event| tap_callback(etype, event),
            ) {
                Ok(t) => break t,
                // 刚授权时 tap 偶尔建不起来，稍后重试。
                Err(()) => std::thread::sleep(Duration::from_secs(2)),
            }
        };
        TAP_PORT.store(tap.mach_port().as_concrete_TypeRef() as *mut _, Ordering::Relaxed);
        let source = tap.mach_port().create_runloop_source(0).expect("runloop source");
        CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
        tap.enable();
        CFRunLoop::run_current();
    });
}
