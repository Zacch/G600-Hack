//! G600 switcher: tray app that puts the right onboard profile on the mouse
//! for whichever app is in the foreground.
//!
//! Config:  %APPDATA%\g600cfg\profiles.toml (created on first start)
//! Log:     %APPDATA%\g600cfg\switcher.log
//! Backups: %APPDATA%\g600cfg\backups\<unix-time>\ (slot contents at each open)

// Release builds run without a console window; debug builds keep one for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod foreground;
mod slots;
mod worker;

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
};

use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIconBuilder,
};
use windows::{
    core::{w, HSTRING},
    Win32::{
        Foundation::{GetLastError, ERROR_ALREADY_EXISTS},
        System::Threading::CreateMutexW,
        UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, MessageBoxW, PostQuitMessage, SetTimer,
            TranslateMessage, MB_ICONERROR, MB_OK, MSG,
        },
    },
};

use worker::{Msg, Paths, Worker};

fn fatal(msg: &str) -> ! {
    unsafe {
        MessageBoxW(None, &HSTRING::from(msg), w!("G600 switcher"), MB_OK | MB_ICONERROR);
    }
    std::process::exit(1);
}

/// A 32x32 round icon: green ring on dark grey.
fn icon() -> Icon {
    let n = 32usize;
    let mut rgba = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let r = (dx * dx + dy * dy).sqrt();
            let px = if r > 15.5 {
                [0, 0, 0, 0]
            } else if r > 11.0 {
                [0x30, 0xD0, 0x60, 0xFF]
            } else {
                [0x30, 0x30, 0x30, 0xFF]
            };
            rgba.extend_from_slice(&px);
        }
    }
    Icon::from_rgba(rgba, n as u32, n as u32).expect("valid icon")
}

/// If `dir` really lives in some app's package folder (Windows redirects
/// %APPDATA% for processes started by a packaged app, e.g. the Claude desktop
/// app), return that app's package name. Other programs can't see such files.
fn redirected_into_package(dir: &Path) -> Option<String> {
    let real = std::fs::canonicalize(dir).ok()?;
    let s = real.to_string_lossy();
    let i = s.to_ascii_lowercase().find("\\appdata\\local\\packages\\")?;
    let rest = &s[i + "\\appdata\\local\\packages\\".len()..];
    // "Claude_pzs8sxrjxfjjc\LocalCache\..." -> "Claude"
    rest.split(['\\', '_']).next().map(str::to_string)
}

fn main() {
    // One instance only: two would fight over the mouse.
    let _mutex = unsafe { CreateMutexW(None, true, w!("Local\\g600-switcher")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        fatal("G600 switcher is already running (see the tray).");
    }

    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| fatal("APPDATA is not set"))
        .join("g600cfg");
    let paths = Paths {
        config: base.join("profiles.toml"),
        log: base.join("switcher.log"),
        backups: base.join("backups"),
    };
    if let Err(e) = std::fs::create_dir_all(&paths.backups) {
        fatal(&format!("cannot create {}: {e}", base.display()));
    }
    if !paths.config.exists() {
        if let Err(e) = std::fs::write(&paths.config, config::STARTER_CONFIG) {
            fatal(&format!("cannot write {}: {e}", paths.config.display()));
        }
    }
    let config_path = paths.config.clone();

    let (tx, rx) = mpsc::channel();
    let (status_tx, status_rx) = mpsc::channel::<String>();
    let worker = thread::spawn(move || Worker::new(paths, status_tx).run(rx));

    let _hook = foreground::install(tx.clone()).unwrap_or_else(|e| fatal(&e));
    if let Some(exe) = foreground::current() {
        let _ = tx.send(Msg::Foreground(exe));
    }

    let status = MenuItem::new("Starting…", false, None);
    let pause = CheckMenuItem::new("Pause switching", true, false, None);
    let edit = MenuItem::new("Edit profiles…", true, None);
    let reload = MenuItem::new("Reload profiles", true, None);
    let folder = MenuItem::new("Open config folder", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    // Started from a packaged app (e.g. the Claude desktop app)? Then Windows
    // redirects our %APPDATA% into that app's private folder, where other
    // programs (Notepad, Explorer) can't see the config.
    let tip_prefix = match redirected_into_package(&base) {
        Some(pkg) => {
            let warn = MenuItem::new(
                format!("⚠ Started from {pkg}: config is redirected there. Start the switcher from Explorer."),
                false,
                None,
            );
            let _ = menu.append_items(&[&warn, &PredefinedMenuItem::separator()]);
            use std::io::Write as _;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(base.join("switcher.log")) {
                let _ = writeln!(f, "WARNING: started from {pkg}; %APPDATA% is redirected into its package folder");
            }
            "⚠ "
        }
        None => "",
    };
    menu.append_items(&[
        &status,
        &PredefinedMenuItem::separator(),
        &pause,
        &edit,
        &reload,
        &folder,
        &PredefinedMenuItem::separator(),
        &quit,
    ])
    .unwrap_or_else(|e| fatal(&format!("menu: {e}")));
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("G600 switcher")
        .with_icon(icon())
        .build()
        .unwrap_or_else(|e| fatal(&format!("tray icon: {e}")));

    // Wake the loop regularly to drain menu events and status updates.
    unsafe { SetTimer(None, 0, 200, None) };
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == *pause.id() {
                let _ = tx.send(Msg::SetPaused(pause.is_checked()));
            } else if ev.id == *edit.id() {
                let _ = Command::new("notepad.exe").arg(&config_path).spawn();
            } else if ev.id == *reload.id() {
                let _ = tx.send(Msg::Reload);
            } else if ev.id == *folder.id() {
                let _ = Command::new("explorer.exe").arg(&base).spawn();
            } else if ev.id == *quit.id() {
                unsafe { PostQuitMessage(0) };
            }
        }
        while let Ok(s) = status_rx.try_recv() {
            status.set_text(&s);
            // Tooltips are limited to 127 characters.
            let tip: String = format!("{tip_prefix}G600: {s}").chars().take(120).collect();
            let _ = tray.set_tooltip(Some(tip));
        }
    }

    // Let the worker finish any write in progress before exiting.
    let _ = tx.send(Msg::Quit);
    let _ = worker.join();
}
