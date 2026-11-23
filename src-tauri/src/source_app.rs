//! 记住「取文字时的前台程序」，替换原文时切回去再粘贴。只记窗口 / 进程号，不读内容。

#[cfg(target_os = "macos")]
mod imp {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
    use std::sync::atomic::{AtomicI32, Ordering};

    static PID: AtomicI32 = AtomicI32::new(0);

    pub fn remember() {
        let pid = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|a| a.processIdentifier())
            .unwrap_or(0);
        if pid != std::process::id() as i32 {
            PID.store(pid, Ordering::Relaxed);
        }
    }

    pub fn activate() -> bool {
        let pid = PID.load(Ordering::Relaxed);
        pid != 0
            && NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
                .is_some_and(|a| a.activateWithOptions(NSApplicationActivationOptions::empty()))
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicIsize, Ordering};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
    };

    static HWND: AtomicIsize = AtomicIsize::new(0);

    pub fn remember() {
        unsafe {
            let fg = GetForegroundWindow();
            let mut pid = 0u32;
            GetWindowThreadProcessId(fg, &mut pid);
            if !fg.is_null() && pid != GetCurrentProcessId() {
                HWND.store(fg as isize, Ordering::Relaxed);
            }
        }
    }

    pub fn activate() -> bool {
        let h = HWND.load(Ordering::Relaxed) as windows_sys::Win32::Foundation::HWND;
        !h.is_null() && unsafe { IsWindow(h) != 0 && SetForegroundWindow(h) != 0 }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod imp {
    pub fn remember() {}
    pub fn activate() -> bool {
        false
    }
}

pub use imp::{activate, remember};
