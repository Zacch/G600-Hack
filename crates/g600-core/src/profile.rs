//! Decoding of one onboard profile report (0xF3/0xF4/0xF5).
//!
//! Expected layout (154 bytes incl. report id), per libratbag:
//! ```text
//!  0      report id
//!  1..4   LED r,g,b
//!  4      LED effect      5  LED duration
//!  6..11  unknown
//! 11      report rate     12 DPI while shifted (x50)   13 default DPI slot (1-4)
//! 14..18  DPI slots (x50, 0 = off)
//! 18..31  unknown
//! 31..91  20 buttons x {code, modifier, key}
//! 91..94  G-Shift LED r,g,b
//! 94..154 20 G-Shift buttons x {code, modifier, key}
//! ```

use crate::keys::{key_name, modifier_names};

pub const PROFILE_REPORT_LEN: usize = 154;
pub const NUM_BUTTONS: usize = 20;
const BUTTONS_OFFSET: usize = 31;
const SHIFT_COLOR_OFFSET: usize = 91;
const SHIFT_BUTTONS_OFFSET: usize = 94;

/// Physical button names, in the order they appear in the report.
pub const BUTTON_NAMES: [&str; NUM_BUTTONS] = [
    "Left", "Right", "Middle", "Tilt-L/Back", "Tilt-R/Fwd", "Ring (G-Shift)", "G7", "G8",
    "G9", "G10", "G11", "G12", "G13", "G14", "G15", "G16", "G17", "G18", "G19", "G20",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Button {
    pub code: u8,
    pub modifier: u8,
    pub key: u8,
}

impl Button {
    /// Interpretation of the raw entry. Codes other than 0x00 are taken from
    /// libratbag and still need confirming against the real mouse.
    pub fn describe(&self) -> String {
        match self.code {
            0x00 if self.key == 0 && self.modifier == 0 => "Disabled".into(),
            0x00 => {
                let mut parts: Vec<String> =
                    modifier_names(self.modifier).into_iter().map(String::from).collect();
                if self.key != 0 {
                    parts.push(key_name(self.key));
                }
                parts.join("+")
            }
            0x01..=0x05 => format!("Mouse button {}", self.code),
            0x11 => "DPI up".into(),
            0x12 => "DPI down".into(),
            0x13 => "DPI cycle".into(),
            0x14 => "Profile cycle".into(),
            0x15 => "DPI shift".into(),
            0x17 => "G-Shift".into(),
            c => format!("Unknown code 0x{c:02X}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub raw: Vec<u8>,
    pub led_rgb: [u8; 3],
    pub report_rate: u8,
    pub dpi_shift: u8,
    pub dpi_default: u8,
    pub dpi: [u8; 4],
    pub buttons: [Button; NUM_BUTTONS],
    pub shift_led_rgb: [u8; 3],
    pub shift_buttons: [Button; NUM_BUTTONS],
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("profile report too short: {0} bytes, expected {PROFILE_REPORT_LEN}")]
    TooShort(usize),
}

fn buttons_at(b: &[u8], off: usize) -> [Button; NUM_BUTTONS] {
    std::array::from_fn(|i| {
        let o = off + i * 3;
        Button { code: b[o], modifier: b[o + 1], key: b[o + 2] }
    })
}

impl Profile {
    pub fn parse(b: &[u8]) -> Result<Self, ParseError> {
        if b.len() < PROFILE_REPORT_LEN {
            return Err(ParseError::TooShort(b.len()));
        }
        Ok(Profile {
            raw: b.to_vec(),
            led_rgb: [b[1], b[2], b[3]],
            report_rate: b[11],
            dpi_shift: b[12],
            dpi_default: b[13],
            dpi: [b[14], b[15], b[16], b[17]],
            buttons: buttons_at(b, BUTTONS_OFFSET),
            shift_led_rgb: [b[SHIFT_COLOR_OFFSET], b[SHIFT_COLOR_OFFSET + 1], b[SHIFT_COLOR_OFFSET + 2]],
            shift_buttons: buttons_at(b, SHIFT_BUTTONS_OFFSET),
        })
    }

    /// Return a copy of the raw report with one button entry replaced.
    /// Every other byte stays exactly as read from the mouse.
    pub fn with_button(&self, index: usize, g_shift: bool, btn: Button) -> Vec<u8> {
        let base = if g_shift { SHIFT_BUTTONS_OFFSET } else { BUTTONS_OFFSET };
        let o = base + index * 3;
        let mut raw = self.raw.clone();
        raw[o..o + 3].copy_from_slice(&[btn.code, btn.modifier, btn.key]);
        raw
    }

    pub fn summary(&self) -> String {
        let dpi: Vec<String> = self
            .dpi
            .iter()
            .map(|&d| if d == 0 { "off".into() } else { (d as u32 * 50).to_string() })
            .collect();
        let mut s = format!(
            "  LED #{:02X}{:02X}{:02X}  G-Shift LED #{:02X}{:02X}{:02X}  rate byte {}\n  DPI slots [{}]  default slot {}  shift-DPI {}\n",
            self.led_rgb[0], self.led_rgb[1], self.led_rgb[2],
            self.shift_led_rgb[0], self.shift_led_rgb[1], self.shift_led_rgb[2],
            self.report_rate, dpi.join(", "), self.dpi_default, self.dpi_shift as u32 * 50,
        );
        s += &format!("  {:<14} {:<26} {}\n", "Button", "Normal", "G-Shift");
        for i in 0..NUM_BUTTONS {
            let (n, g) = (self.buttons[i], self.shift_buttons[i]);
            s += &format!(
                "  {:<14} {:<26} {:<26} raw {:02X} {:02X} {:02X} | {:02X} {:02X} {:02X}\n",
                BUTTON_NAMES[i], n.describe(), g.describe(),
                n.code, n.modifier, n.key, g.code, g.modifier, g.key,
            );
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_with_modifier() {
        let mut b = vec![0u8; PROFILE_REPORT_LEN];
        b[0] = 0xF3;
        // G9 = LShift+W
        let o = BUTTONS_OFFSET + 8 * 3;
        b[o..o + 3].copy_from_slice(&[0x00, 0x02, 0x1A]);
        let p = Profile::parse(&b).unwrap();
        assert_eq!(p.buttons[8].describe(), "LShift+W");
        assert_eq!(p.buttons[0].describe(), "Disabled");
    }

    #[test]
    fn rejects_short() {
        assert!(Profile::parse(&[0xF3; 10]).is_err());
    }
}

/// Look up a button by name ("G9", "g9", "Left", "G7"...).
pub fn button_index(name: &str) -> Option<usize> {
    BUTTON_NAMES.iter().position(|n| {
        n.eq_ignore_ascii_case(name)
            || n.split(['/', ' ']).next().is_some_and(|p| p.eq_ignore_ascii_case(name))
    })
}

#[cfg(test)]
mod edit_tests {
    use super::*;

    #[test]
    fn with_button_changes_only_three_bytes() {
        let raw: Vec<u8> = (0..PROFILE_REPORT_LEN).map(|i| i as u8).collect();
        let p = Profile::parse(&raw).unwrap();
        let btn = Button { code: 0, modifier: 0x02, key: 0x1A };
        let out = p.with_button(button_index("g9").unwrap(), false, btn);
        let diff: Vec<usize> = (0..raw.len()).filter(|&i| raw[i] != out[i]).collect();
        assert_eq!(diff, vec![55, 56, 57]); // 31 + 8*3
        assert_eq!(Profile::parse(&out).unwrap().buttons[8], btn);
    }

    #[test]
    fn button_lookup() {
        assert_eq!(button_index("G9"), Some(8));
        assert_eq!(button_index("left"), Some(0));
        assert_eq!(button_index("Ring"), Some(5));
        assert_eq!(button_index("G21"), None);
    }
}
