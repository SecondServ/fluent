//! macOS：注册系统「服务」，让选中文字的右键菜单里出现「用 Fluent 翻译」。
//!
//! 服务在 Info.plist 的 NSServices 里声明（NSMessage = "translate"），系统在用户点菜单项时
//! 调用 servicesProvider 的 `translate:userData:error:`，并把选中的文字放在专用剪贴板里传进来。
//! 不碰用户的通用剪贴板，也不需要「辅助功能」权限，更不做任何监听。

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSPasteboard, NSPasteboardTypeString, NSRegisterServicesProvider, NSUpdateDynamicServices,
};
use objc2_foundation::{NSObjectProtocol, NSString, ns_string};
use std::sync::OnceLock;

type Handler = Box<dyn Fn(String) + Send + Sync>;
static HANDLER: OnceLock<Handler> = OnceLock::new();

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "FluentServiceProvider"]
    struct ServiceProvider;

    unsafe impl NSObjectProtocol for ServiceProvider {}

    impl ServiceProvider {
        #[unsafe(method(translate:userData:error:))]
        fn translate(&self, pboard: &NSPasteboard, _user_data: Option<&NSString>, _error: *mut *mut NSString) {
            let text = unsafe { pboard.stringForType(NSPasteboardTypeString) }
                .map(|s| s.to_string())
                .unwrap_or_default();
            if let Some(h) = HANDLER.get() {
                h(text);
            }
        }
    }
);

/// 必须在主线程调用（Tauri 的 setup 就在主线程）。
pub fn register(handler: impl Fn(String) + Send + Sync + 'static) {
    let _ = HANDLER.set(Box::new(handler));
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("services: not on main thread");
        return;
    };
    let provider: Retained<ServiceProvider> = unsafe { msg_send![ServiceProvider::alloc(mtm), init] };
    let app = NSApplication::sharedApplication(mtm);
    // SAFETY: provider 实现了服务方法，且下面 forget 让它在整个进程生命期内有效。
    unsafe { app.setServicesProvider(Some(&provider)) };
    // 显式用 Info.plist 里的 NSPortName 注册端口。
    let obj: &AnyObject = &provider;
    unsafe { NSRegisterServicesProvider(Some(obj), ns_string!("Fluent")) };
    // NSApp 对 servicesProvider 是弱引用，这里让它常驻。
    std::mem::forget(provider);
    NSUpdateDynamicServices();
}

