//! Finding the G600, reading feature reports, and writing them safely.
//!
//! Write safety rules (learned the hard way, and matching findings in
//! ka2iba/device-hub docs/G600_BRIEF.md §3.5–3.8). Breaking them made the
//! firmware save profiles with byte 3 duplicated at offset 4 and the rest shifted:
//! 1. Wait 2 s after opening the device before any SET.
//! 2. Before a SET, poll report 0xF0 until the idle bit (b1 & 0x08) is set.
//! 3. Send exactly one SET, wait 300 ms, then poll until idle again.
//!    Send nothing else while the mouse is busy saving.
//! 4. Never write the profile that is currently active; select another first.
//! 5. A read right after a write may echo what was sent, not what was saved;
//!    only a USB replug proves what was stored.

use std::{
    thread::sleep,
    time::{Duration, Instant},
};

use hidapi::{DeviceInfo, HidApi, HidDevice};

use crate::{PRODUCT_ID, PROFILE_REPORT_IDS, REPORT_ID_ACTIVE, VENDOR_ID};

pub use hidapi::HidError;

const SETTLE_AFTER_OPEN: Duration = Duration::from_secs(2);
const POST_WRITE_DELAY: Duration = Duration::from_millis(300);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const IDLE_TIMEOUT: Duration = Duration::from_secs(3);
const IDLE_BIT: u8 = 0x08;

/// All HID collections the G600 exposes (mouse, keyboard, vendor-defined...).
pub fn collections(api: &HidApi) -> Vec<&DeviceInfo> {
    api.device_list()
        .filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID)
        .collect()
}

/// Decoded report 0xF0.
#[derive(Debug, Clone, Copy)]
pub struct Status {
    pub raw: u8,
    /// Active profile, 0-based (0 = report 0xF3).
    pub profile: u8,
    /// Current DPI slot, 0-based.
    pub resolution: u8,
    /// False while the mouse is busy saving a write.
    pub idle: bool,
}

impl Status {
    pub fn from_byte(b: u8) -> Self {
        Status { raw: b, profile: (b >> 4) & 0x0F, resolution: (b >> 1) & 0x03, idle: b & IDLE_BIT != 0 }
    }
}

pub struct G600 {
    dev: HidDevice,
    opened_at: Instant,
    pub path: String,
}

impl G600 {
    /// Open the collection that answers the profile feature report.
    /// Windows locks the mouse/keyboard collections, so we try each one.
    pub fn open(api: &HidApi) -> Result<Self, String> {
        let mut errors = Vec::new();
        for info in collections(api) {
            let path = info.path().to_string_lossy().into_owned();
            match info.open_device(api) {
                Ok(dev) => {
                    let g = G600 { dev, opened_at: Instant::now(), path: path.clone() };
                    match g.read_report(PROFILE_REPORT_IDS[0]) {
                        Ok(_) => return Ok(g),
                        Err(e) => errors.push(format!("{path}: read failed: {e}")),
                    }
                }
                Err(e) => errors.push(format!("{path}: open failed: {e}")),
            }
        }
        if errors.is_empty() {
            Err("no G600 found (VID 046D, PID C24A)".into())
        } else {
            Err(format!("no G600 collection answered:\n  {}", errors.join("\n  ")))
        }
    }

    /// Read a feature report; the returned bytes start with the report id.
    pub fn read_report(&self, id: u8) -> Result<Vec<u8>, HidError> {
        let mut buf = [0u8; 256];
        buf[0] = id;
        let n = self.dev.get_feature_report(&mut buf)?;
        Ok(buf[..n].to_vec())
    }

    pub fn status(&self) -> Result<Status, String> {
        let b = self.read_report(REPORT_ID_ACTIVE).map_err(|e| format!("read 0xF0: {e}"))?;
        b.get(1).map(|&x| Status::from_byte(x)).ok_or_else(|| "0xF0 report too short".into())
    }

    fn wait_idle(&self, what: &str) -> Result<Status, String> {
        let start = Instant::now();
        loop {
            let s = self.status()?;
            if s.idle {
                return Ok(s);
            }
            if start.elapsed() > IDLE_TIMEOUT {
                return Err(format!("mouse still busy {what} after {IDLE_TIMEOUT:?} (0xF0 b1 = 0x{:02X})", s.raw));
            }
            sleep(POLL_INTERVAL);
        }
    }

    /// One SET following the safety rules in the module docs.
    fn set_report_safe(&self, data: &[u8]) -> Result<(), String> {
        if let Some(rest) = SETTLE_AFTER_OPEN.checked_sub(self.opened_at.elapsed()) {
            sleep(rest);
        }
        self.wait_idle("before write")?;
        self.dev
            .send_feature_report(data)
            .map_err(|e| format!("write 0x{:02X}: {e}", data[0]))?;
        sleep(POST_WRITE_DELAY);
        self.wait_idle("after write")?;
        Ok(())
    }

    /// Make profile `index` (0-2) active.
    pub fn select_profile(&self, index: u8) -> Result<(), String> {
        if index > 2 {
            return Err(format!("profile index {index} out of range"));
        }
        self.set_report_safe(&[REPORT_ID_ACTIVE, 0x80 | (index << 4), 0, 0])
    }

    /// Re-apply DPI slot `index` (0-3); needed for stored DPI changes to take effect.
    pub fn apply_resolution(&self, index: u8) -> Result<(), String> {
        if index > 3 {
            return Err(format!("resolution index {index} out of range"));
        }
        self.set_report_safe(&[REPORT_ID_ACTIVE, 0x40 | (index << 1), 0, 0])
    }

    /// Write a full profile report (data[0] = 0xF3/F4/F5). Refuses to write
    /// the active profile. Returns whether the immediate read-back matched
    /// (a hint only: replug and read again to confirm what was stored).
    pub fn write_profile(&self, data: &[u8]) -> Result<bool, String> {
        let slot = PROFILE_REPORT_IDS
            .iter()
            .position(|&id| id == data[0])
            .ok_or_else(|| format!("0x{:02X} is not a profile report", data[0]))?;
        let active = self.status()?.profile as usize;
        if slot == active {
            return Err(format!(
                "profile {} is active; select another profile before writing it",
                slot + 1
            ));
        }
        self.set_report_safe(data)?;
        let back = self.read_report(data[0]).map_err(|e| format!("read-back: {e}"))?;
        Ok(back == data)
    }
}

#[cfg(test)]
mod tests {
    use super::Status;

    #[test]
    fn decodes_status() {
        let s = Status::from_byte(0x19);
        assert_eq!((s.profile, s.resolution, s.idle), (1, 0, true));
        let s = Status::from_byte(0x00);
        assert!(!s.idle);
        let s = Status::from_byte(0x08);
        assert_eq!((s.profile, s.idle), (0, true));
    }
}
