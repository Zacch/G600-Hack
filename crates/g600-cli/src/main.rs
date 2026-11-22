//! g600 — probe tool. All commands are read-only.
//!
//!   g600 list   show the HID collections the mouse exposes
//!   g600 dump   read active-profile + profile reports, decode them,
//!               and save raw copies under backups/<unix-time>/

use std::{fs, path::PathBuf, process::ExitCode, time::SystemTime};

use g600_core::{device, profile::Profile, PROFILE_REPORT_IDS, REPORT_ID_ACTIVE};
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

fn list(api: &HidApi) {
    let cols = device::collections(api);
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
}

fn dump(api: &HidApi) -> Result<(), String> {
    let g = device::G600::open(api)?;
    println!("Opened {}\n", g.path);

    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dir = PathBuf::from("backups").join(stamp.to_string());
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;

    match g.read_report(REPORT_ID_ACTIVE) {
        Ok(b) => {
            println!("Report 0x{REPORT_ID_ACTIVE:02X} (active profile / DPI), {} bytes:\n{}\n", b.len(), hex(&b));
            let _ = fs::write(dir.join(format!("{REPORT_ID_ACTIVE:02X}.bin")), &b);
        }
        Err(e) => println!("Report 0x{REPORT_ID_ACTIVE:02X}: {e}\n"),
    }

    for (slot, &id) in PROFILE_REPORT_IDS.iter().enumerate() {
        let b = g.read_report(id).map_err(|e| format!("read 0x{id:02X}: {e}"))?;
        fs::write(dir.join(format!("{id:02X}.bin")), &b).map_err(|e| e.to_string())?;
        println!("=== Profile {} (report 0x{id:02X}), {} bytes ===\n{}", slot + 1, b.len(), hex(&b));
        match Profile::parse(&b) {
            Ok(p) => println!("{}", p.summary()),
            Err(e) => println!("  decode failed: {e}\n"),
        }
    }
    println!("Raw reports saved to {}", dir.display());
    Ok(())
}

fn main() -> ExitCode {
    let api = match HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("hidapi init failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    match std::env::args().nth(1).as_deref() {
        Some("list") => list(&api),
        Some("dump") => {
            if let Err(e) = dump(&api) {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        }
        _ => {
            eprintln!("usage: g600 <list|dump>");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
