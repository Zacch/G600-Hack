//! Which app is in the foreground: a WinEvent hook for changes, plus a
//! one-off query. The hook needs a message loop on the installing thread.

use std::sync::{mpsc::Sender, OnceLock};

use g600_app::apps::exe_of_window;
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK},
        WindowsAndMessaging::{
            GetForegroundWindow, EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
            WINEVENT_SKIPOWNPROCESS,
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

