//! g600 — probe tool.
//!
//!   g600 list                            show the HID collections the mouse exposes
//!   g600 dump                            decode all profiles (also saves a backup)
//!   g600 set <1-3> <button> <combo> [--shift]
//!                                        e.g. `g600 set 1 G9 Shift+W`; backs up first
//!   g600 status                          active profile, DPI slot, busy flag
//!   g600 select <1-3>                    make a profile active (+ apply its DPI)
//!   g600 restore <backup-dir> [--slot N] write backed-up profiles (skips the active one)
//!
//! Writes follow the safety rules in g600_core::device.

use std::{fs, path::{Path, PathBuf}, process::ExitCode, time::SystemTime};

use g600_core::{
    device::G600,
    keys::parse_combo,
    profile::{button_index, Button, Profile, BUTTON_NAMES, PROFILE_REPORT_LEN},
    PROFILE_REPORT_IDS, REPORT_ID_ACTIVE,
};
use hidapi::HidApi;

fn hex(b: &[u8]) -> String {
    b.chunks(16)
        .enumerate()
        .map(|(i, c)| {
            let h: Vec<String> = c.iter().map(|x| format!("{x:02X}")).collect();
            format!("    {:03X}: {}", i * 16, h.join(" "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn list(api: &HidApi) -> Result<(), String> {
    let cols = g600_core::device::collections(api);
    if cols.is_empty() {
        println!("No G600 found.");
    }
    for d in cols {
        println!(
            "interface {}  usage_page 0x{:04X}  usage 0x{:04X}\n  {}",
            d.interface_number(),
            d.usage_page(),
            d.usage(),
            d.path().to_string_lossy()
        );
    }
    Ok(())
}

/// Save F0 and F3-F5 to backups/<unix-time>/. Returns the directory.
fn backup(g: &G600) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dir = PathBuf::from("backups").join(stamp.to_string());
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    if let Ok(b) = g.read_report(REPORT_ID_ACTIVE) {
        fs::write(dir.join(format!("{REPORT_ID_ACTIVE:02X}.bin")), &b).map_err(|e| e.to_string())?;
    }
    for id in PROFILE_REPORT_IDS {
        let b = g.read_report(id).map_err(|e| format!("read 0x{id:02X}: {e}"))?;
        fs::write(dir.join(format!("{id:02X}.bin")), &b).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

fn dump(api: &HidApi) -> Result<(), String> {
    let g = G600::open(api)?;
    println!("Opened {}\n", g.path);
    match g.read_report(REPORT_ID_ACTIVE) {
        Ok(b) => println!("Report 0x{REPORT_ID_ACTIVE:02X} (active profile / DPI), {} bytes:\n{}\n", b.len(), hex(&b)),
        Err(e) => println!("Report 0x{REPORT_ID_ACTIVE:02X}: {e}\n"),
    }
    for (slot, &id) in PROFILE_REPORT_IDS.iter().enumerate() {
        let b = g.read_report(id).map_err(|e| format!("read 0x{id:02X}: {e}"))?;
        println!("=== Profile {} (report 0x{id:02X}), {} bytes ===\n{}", slot + 1, b.len(), hex(&b));
        match Profile::parse(&b) {
            Ok(p) => println!("{}", p.summary()),
            Err(e) => println!("  decode failed: {e}\n"),
        }
    }
    println!("Backup saved to {}", backup(&g)?.display());
    Ok(())
}

fn set(api: &HidApi, args: &[String]) -> Result<(), String> {
    let usage = "usage: g600 set <1-3> <button> <combo> [--shift]";
    let shift = args.iter().any(|a| a == "--shift");
    let pos: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let [slot, button, combo] = pos.as_slice() else { return Err(usage.into()) };
    let slot: usize = slot.parse().ok().filter(|s| (1..=3).contains(s)).ok_or(usage)?;
    let index = button_index(button)
        .ok_or_else(|| format!("unknown button '{button}'; known: {}", BUTTON_NAMES.join(", ")))?;
    let (modifier, key) = parse_combo(combo)?;
    let new = Button { code: 0x00, modifier, key };

    let g = G600::open(api)?;
    let dir = backup(&g)?;
    println!("Backup saved to {}", dir.display());

    let id = PROFILE_REPORT_IDS[slot - 1];
    let raw = g.read_report(id).map_err(|e| format!("read 0x{id:02X}: {e}"))?;
    let profile = Profile::parse(&raw).map_err(|e| e.to_string())?;
    let old = if shift { profile.shift_buttons[index] } else { profile.buttons[index] };
    let data = profile.with_button(index, shift, new);

    println!(
        "Profile {slot}, {}{}: {} -> {}",
        BUTTON_NAMES[index],
        if shift { " (G-Shift)" } else { "" },
        old.describe(),
        new.describe()
    );
    report_write(g.write_profile(&data)?);
    Ok(())
}

fn report_write(readback_matched: bool) {
    if readback_matched {
        println!("Written; immediate read-back matches. Replug the mouse and run `g600 dump` to confirm it was stored.");
    } else {
        println!("Written, but immediate read-back DIFFERS. Do not write more; replug and run `g600 dump`.");
    }
}

fn status(api: &HidApi) -> Result<(), String> {
    let g = G600::open(api)?;
    let s = g.status()?;
    println!(
        "0xF0 b1 = 0x{:02X}: active profile {}, DPI slot {}, {}",
        s.raw,
        s.profile + 1,
        s.resolution + 1,
        if s.idle { "idle" } else { "BUSY saving" }
    );
    Ok(())
}

fn select(api: &HidApi, args: &[String]) -> Result<(), String> {
    let usage = "usage: g600 select <1-3>";
    let p: u8 = args.first().and_then(|a| a.parse().ok()).filter(|p| (1..=3).contains(p)).ok_or(usage)?;
    let g = G600::open(api)?;
    g.select_profile(p - 1)?;
    let s = g.status()?;
    println!("Selected profile {p}; mouse reports active profile {}.", s.profile + 1);
    // Make the profile's stored default DPI slot take effect.
    let raw = g.read_report(PROFILE_REPORT_IDS[(p - 1) as usize]).map_err(|e| e.to_string())?;
    if let Ok(prof) = Profile::parse(&raw) {
        if (1..=4).contains(&prof.dpi_default) {
            g.apply_resolution(prof.dpi_default - 1)?;
            println!("Applied default DPI slot {}.", prof.dpi_default);
        }
    }
    Ok(())
}

fn restore(api: &HidApi, args: &[String]) -> Result<(), String> {
    let usage = "usage: g600 restore <backup-dir> [--slot N]...";
    let mut dir = None;
    let mut slots = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--slot" {
            let n: usize = it.next().and_then(|s| s.parse().ok()).filter(|n| (1..=3).contains(n)).ok_or(usage)?;
            slots.push(PROFILE_REPORT_IDS[n - 1]);
        } else {
            dir = Some(Path::new(a));
        }
    }
    let dir = dir.ok_or(usage)?;
    if slots.is_empty() {
        slots = PROFILE_REPORT_IDS.to_vec();
    }
    // Load and validate everything before touching the mouse.
    let mut reports = Vec::new();
    for id in slots {
        let f = dir.join(format!("{id:02X}.bin"));
        let b = fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        if b.len() != PROFILE_REPORT_LEN || b[0] != id {
            return Err(format!("{}: not a valid 0x{id:02X} profile report", f.display()));
        }
        reports.push(b);
    }
    let g = G600::open(api)?;
    println!("Safety backup of current state: {}", backup(&g)?.display());
    let active = PROFILE_REPORT_IDS[g.status()?.profile as usize];
    for b in &reports {
        let current = g.read_report(b[0]).map_err(|e| e.to_string())?;
        if &current == b {
            println!("0x{:02X}: already identical, skipped", b[0]);
        } else if b[0] == active {
            println!("0x{:02X}: SKIPPED, it is the active profile (select another one first)", b[0]);
        } else {
            print!("0x{:02X}: ", b[0]);
            report_write(g.write_profile(b)?);
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let api = match HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("hidapi init failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let rest = args.get(1..).unwrap_or(&[]);
    let result = match args.first().map(String::as_str) {
        Some("list") => list(&api),
        Some("dump") => dump(&api),
        Some("set") => set(&api, rest),
        Some("restore") => restore(&api, rest),
        Some("status") => status(&api),
        Some("select") => select(&api, rest),
        _ => Err("usage: g600 <list|dump|status|select|set|restore>".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
