//! Which app is in the foreground: a WinEvent hook for changes, plus a
//! one-off query. The hook needs a message loop on the installing thread.

use std::sync::{mpsc::Sender, OnceLock};

use windows::{
    core::PWSTR,
    Win32::{
        Foundation::{CloseHandle, HWND},
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::{
            Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK},
            WindowsAndMessaging::{
                GetForegroundWindow, GetWindowThreadProcessId, EVENT_SYSTEM_FOREGROUND,
                WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
            },
        },
    },
};

use crate::worker::Msg;

static SINK: OnceLock<Sender<Msg>> = OnceLock::new();

pub struct Hook(HWINEVENTHOOK);

impl Drop for Hook {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWinEvent(self.0);
        }
    }
}

/// Start reporting foreground changes as `Msg::Foreground(exe)`.
pub fn install(tx: Sender<Msg>) -> Result<Hook, String> {
    SINK.set(tx).map_err(|_| "foreground hook installed twice".to_string())?;
    let h = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    if h.is_invalid() {
        return Err("SetWinEventHook failed".into());
    }
    Ok(Hook(h))
}

unsafe extern "system" fn on_foreground(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if let (Some(exe), Some(tx)) = (exe_of_window(hwnd), SINK.get()) {
        let _ = tx.send(Msg::Foreground(exe));
    }
}

/// Exe file name of the current foreground window.
pub fn current() -> Option<String> {
    exe_of_window(unsafe { GetForegroundWindow() })
}

/// Exe file name ("notepad.exe") owning a window. None for windows of
/// processes we may not query (e.g. elevated ones when we are not).
fn exe_of_window(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit('\\').next().map(str::to_string)
    }
}
