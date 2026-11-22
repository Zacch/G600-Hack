//! One onboard profile (feature report 0xF3/0xF4/0xF5), typed both ways.
//!
//! Layout (154 bytes incl. report id), confirmed against real dumps:
//! ```text
//!  0      report id (F3/F4/F5)
//!  1..4   LED r,g,b          4  LED effect       5  LED duration
//!  6..11  unknown (kept)
//! 11      report rate (1000 / (n+1) Hz)
//! 12      DPI-shift DPI /50  13 default DPI slot (1-4)
//! 14..18  DPI slots /50 (0 = off)
//! 18..31  unknown (kept; byte 24 is 0x02 in every dump so far)
//! 31..91  20 buttons x {code, modifier, key}
//! 91..94  G-Shift LED r,g,b
//! 94..154 20 G-Shift buttons
//! ```
//! `Profile::decode(b).encode() == b` holds for every valid report.

use std::fmt::Write as _;

use crate::{action::Action, PROFILE_REPORT_IDS};

pub const PROFILE_REPORT_LEN: usize = 154;
pub const NUM_BUTTONS: usize = 20;
const BUTTONS_OFFSET: usize = 31;
const SHIFT_COLOR_OFFSET: usize = 91;
const SHIFT_BUTTONS_OFFSET: usize = 94;
const FACTORY_REPORT: &[u8] = include_bytes!("../tests/fixtures/original_F3.bin");

/// Physical button names, in report order.
pub const BUTTON_NAMES: [&str; NUM_BUTTONS] = [
    "Left", "Right", "Middle", "Tilt-L/Back", "Tilt-R/Fwd", "Ring (G-Shift)", "G7", "G8",
    "G9", "G10", "G11", "G12", "G13", "G14", "G15", "G16", "G17", "G18", "G19", "G20",
];

/// Look up a button by name ("G9", "g9", "Left", "Ring"...).
pub fn button_index(name: &str) -> Option<usize> {
    BUTTON_NAMES.iter().position(|n| {
        n.eq_ignore_ascii_case(name)
            || n.split(['/', ' ']).next().is_some_and(|p| p.eq_ignore_ascii_case(name))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Led {
    pub color: Rgb,
    /// Raw effect byte (libratbag: 0 solid, 1 breathe, 2 cycle).
    pub effect: u8,
    pub duration: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// Report id 0xF3/0xF4/0xF5, i.e. which slot this profile lives in.
    pub report_id: u8,
    pub led: Led,
    /// Raw report-rate byte; see `report_rate_hz`.
    pub report_rate: u8,
    /// DPI used while a `DpiShift` button is held (0 = none).
    pub dpi_shift: u16,
    /// Default DPI slot, 1-based as stored (0 seen only in corrupted data).
    pub dpi_default: u8,
    /// DPI per slot, 0 = slot off.
    pub dpi: [u16; 4],
    pub buttons: [Action; NUM_BUTTONS],
    pub shift_color: Rgb,
    pub shift_buttons: [Action; NUM_BUTTONS],
    unknown1: [u8; 5],
    unknown2: [u8; 13],
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProfileError {
    #[error("profile report is {0} bytes, expected {PROFILE_REPORT_LEN}")]
    WrongLength(usize),
    #[error("0x{0:02X} is not a profile report id")]
    WrongId(u8),
    #[error("DPI {0} invalid: must be 200-8200 in steps of 50 (or 0 = off)")]
    BadDpi(u16),
}

fn actions_at(b: &[u8], off: usize) -> [Action; NUM_BUTTONS] {
    std::array::from_fn(|i| {
        let o = off + i * 3;
        Action::decode([b[o], b[o + 1], b[o + 2]])
    })
}

impl Profile {
    pub fn decode(b: &[u8]) -> Result<Self, ProfileError> {
        if b.len() != PROFILE_REPORT_LEN {
            return Err(ProfileError::WrongLength(b.len()));
        }
        if !PROFILE_REPORT_IDS.contains(&b[0]) {
            return Err(ProfileError::WrongId(b[0]));
        }
        Ok(Profile {
            report_id: b[0],
            led: Led { color: Rgb(b[1], b[2], b[3]), effect: b[4], duration: b[5] },
            unknown1: b[6..11].try_into().unwrap(),
            report_rate: b[11],
            dpi_shift: b[12] as u16 * 50,
            dpi_default: b[13],
            dpi: std::array::from_fn(|i| b[14 + i] as u16 * 50),
            unknown2: b[18..31].try_into().unwrap(),
            buttons: actions_at(b, BUTTONS_OFFSET),
            shift_color: Rgb(b[91], b[92], b[93]),
            shift_buttons: actions_at(b, SHIFT_BUTTONS_OFFSET),
        })
    }

    /// Build the 154-byte report. Fails only on DPI values the mouse can't store.
    pub fn encode(&self) -> Result<[u8; PROFILE_REPORT_LEN], ProfileError> {
        let dpi_byte = |d: u16| -> Result<u8, ProfileError> {
            // Decoded values are always n*50 with n <= 255, so they round-trip;
            // new values must be in the documented range.
            if d % 50 != 0 || d > 255 * 50 {
                return Err(ProfileError::BadDpi(d));
            }
            Ok((d / 50) as u8)
        };
        let mut b = [0u8; PROFILE_REPORT_LEN];
        b[0] = self.report_id;
        let Rgb(r, g, bl) = self.led.color;
        b[1..6].copy_from_slice(&[r, g, bl, self.led.effect, self.led.duration]);
        b[6..11].copy_from_slice(&self.unknown1);
        b[11] = self.report_rate;
        b[12] = dpi_byte(self.dpi_shift)?;
        b[13] = self.dpi_default;
        for (i, &d) in self.dpi.iter().enumerate() {
            b[14 + i] = dpi_byte(d)?;
        }
        b[18..31].copy_from_slice(&self.unknown2);
        for (i, a) in self.buttons.iter().enumerate() {
            let o = BUTTONS_OFFSET + i * 3;
            b[o..o + 3].copy_from_slice(&a.encode());
        }
        let Rgb(r, g, bl) = self.shift_color;
        b[SHIFT_COLOR_OFFSET..SHIFT_COLOR_OFFSET + 3].copy_from_slice(&[r, g, bl]);
        for (i, a) in self.shift_buttons.iter().enumerate() {
            let o = SHIFT_BUTTONS_OFFSET + i * 3;
            b[o..o + 3].copy_from_slice(&a.encode());
        }
        Ok(b)
    }

    /// The mouse's original profile 1 (as dumped from this G600): left/right/
    /// middle/back/forward, ring = G-Shift, G7 = Shift+B, G8 = profile cycle,
    /// G9-G20 = 1..0,-,= and Ctrl+ those on the G-Shift layer, 1200 DPI.
    /// Used as the template that configured profiles are built on.
    pub fn factory() -> Profile {
        Profile::decode(FACTORY_REPORT).expect("embedded factory profile is valid")
    }

    /// True if the normal (non-G-Shift) layer has a left click somewhere.
    /// Never write a profile without one: the user could not click anymore.
    pub fn has_left_click(&self) -> bool {
        self.buttons.contains(&Action::Mouse(1))
    }

    /// The report bytes without the report id, for comparing profile
    /// contents regardless of which slot they live in.
    pub fn content(&self) -> Result<[u8; PROFILE_REPORT_LEN - 1], ProfileError> {
        let b = self.encode()?;
        Ok(b[1..].try_into().unwrap())
    }

    /// 0-based slot (0 = report 0xF3).
    pub fn slot(&self) -> usize {
        (self.report_id - PROFILE_REPORT_IDS[0]) as usize
    }

    pub fn report_rate_hz(&self) -> u32 {
        1000 / (self.report_rate as u32 + 1)
    }

    /// Set a DPI slot (1-4) with validation; 0 turns the slot off.
    pub fn set_dpi(&mut self, slot: usize, dpi: u16) -> Result<(), ProfileError> {
        if dpi != 0 && (dpi % 50 != 0 || !(200..=8200).contains(&dpi)) {
            return Err(ProfileError::BadDpi(dpi));
        }
        self.dpi[slot - 1] = dpi;
        Ok(())
    }

    pub fn button_mut(&mut self, index: usize, g_shift: bool) -> &mut Action {
        if g_shift { &mut self.shift_buttons[index] } else { &mut self.buttons[index] }
    }

    pub fn summary(&self) -> String {
        let dpi: Vec<String> =
            self.dpi.iter().map(|&d| if d == 0 { "off".into() } else { d.to_string() }).collect();
        let Rgb(r, g, b) = self.led.color;
        let Rgb(sr, sg, sb) = self.shift_color;
        let mut s = String::new();
        let _ = writeln!(
            s,
            "  LED #{r:02X}{g:02X}{b:02X} effect {} duration {}  G-Shift LED #{sr:02X}{sg:02X}{sb:02X}  {} Hz",
            self.led.effect, self.led.duration, self.report_rate_hz()
        );
        let _ = writeln!(
            s,
            "  DPI slots [{}]  default slot {}  shift-DPI {}",
            dpi.join(", "), self.dpi_default, self.dpi_shift
        );
        let _ = writeln!(s, "  {:<15} {:<24} G-Shift", "Button", "Normal");
        for i in 0..NUM_BUTTONS {
            let _ = writeln!(
                s,
                "  {:<15} {:<24} {}",
                BUTTON_NAMES[i],
                self.buttons[i].to_string(),
                self.shift_buttons[i]
            );
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> Vec<u8> {
        let mut b = vec![0u8; PROFILE_REPORT_LEN];
        b[0] = 0xF3;
        b
    }

    #[test]
    fn edit_changes_only_that_button() {
        let raw: Vec<u8> = (0..PROFILE_REPORT_LEN).map(|i| i as u8).collect::<Vec<_>>();
        let mut raw = raw;
        raw[0] = 0xF3;
        let mut p = Profile::decode(&raw).unwrap();
        *p.button_mut(button_index("g9").unwrap(), false) = "Shift+W".parse().unwrap();
        let out = p.encode().unwrap();
        let diff: Vec<usize> = (0..raw.len()).filter(|&i| raw[i] != out[i]).collect();
        assert_eq!(diff, vec![55, 56, 57]); // 31 + 8*3
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(Profile::decode(&[0xF3; 10]), Err(ProfileError::WrongLength(10)));
        let mut b = blank();
        b[0] = 0xF0;
        assert_eq!(Profile::decode(&b), Err(ProfileError::WrongId(0xF0)));
        let mut p = Profile::decode(&blank()).unwrap();
        assert!(p.set_dpi(1, 1234).is_err());
        assert!(p.set_dpi(1, 100).is_err());
        p.set_dpi(1, 1600).unwrap();
        assert_eq!(p.encode().unwrap()[14], 32);
    }

    #[test]
    fn button_lookup() {
        assert_eq!(button_index("G9"), Some(8));
        assert_eq!(button_index("left"), Some(0));
        assert_eq!(button_index("Ring"), Some(5));
        assert_eq!(button_index("G21"), None);
    }
}
