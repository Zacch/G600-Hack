//! HID keyboard usage codes and modifier bits (USB HID Usage Tables, page 0x07).
//!
//! Key names never contain '+', so combos like "LCtrl+NumPlus" split cleanly.

/// Modifier bit names, bit 0 first (standard HID keyboard modifier byte).
pub const MOD_NAMES: [&str; 8] = [
    "LCtrl", "LShift", "LAlt", "LWin", "RCtrl", "RShift", "RAlt", "RWin",
];

pub fn modifier_names(mask: u8) -> Vec<&'static str> {
    (0..8).filter(|b| mask & (1 << b) != 0).map(|b| MOD_NAMES[b]).collect()
}

/// Human-readable name of a HID keyboard usage code (US layout labels).
/// Codes without a name get "Key(0xNN)", which `key_code` also accepts.
pub fn key_name(code: u8) -> String {
    let s = match code {
        0x00 => "None",
        0x04..=0x1D => return ((b'A' + code - 0x04) as char).to_string(),
        0x1E..=0x26 => return ((b'1' + code - 0x1E) as char).to_string(),
        0x27 => "0",
        0x28 => "Enter",
        0x29 => "Esc",
        0x2A => "Backspace",
        0x2B => "Tab",
        0x2C => "Space",
        0x2D => "-",
        0x2E => "=",
        0x2F => "[",
        0x30 => "]",
        0x31 => "\\",
        0x32 => "NonUS#",
        0x33 => ";",
        0x34 => "'",
        0x35 => "`",
        0x36 => ",",
        0x37 => ".",
        0x38 => "/",
        0x39 => "CapsLock",
        0x3A..=0x45 => return format!("F{}", code - 0x3A + 1),
        0x46 => "PrintScreen",
        0x47 => "ScrollLock",
        0x48 => "Pause",
        0x49 => "Insert",
        0x4A => "Home",
        0x4B => "PageUp",
        0x4C => "Delete",
        0x4D => "End",
        0x4E => "PageDown",
        0x4F => "Right",
        0x50 => "Left",
        0x51 => "Down",
        0x52 => "Up",
        0x53 => "NumLock",
        0x54 => "NumSlash",
        0x55 => "NumStar",
        0x56 => "NumMinus",
        0x57 => "NumPlus",
        0x58 => "NumEnter",
        0x59..=0x61 => return format!("Num{}", code - 0x58),
        0x62 => "Num0",
        0x63 => "NumDot",
        0x64 => "NonUS\\",
        0x65 => "Menu",
        0x68..=0x73 => return format!("F{}", code - 0x68 + 13),
        _ => return format!("Key(0x{code:02X})"),
    };
    s.to_string()
}

/// Inverse of `key_name` (case-insensitive).
pub fn key_code(name: &str) -> Option<u8> {
    (1..=0xFFu8).find(|&c| key_name(c).eq_ignore_ascii_case(name))
}

/// Modifier bit for a name: "LShift", "RAlt"..., or the aliases
/// "Shift"/"Ctrl"/"Control"/"Alt"/"Win"/"Gui", which mean the left key.
pub fn modifier_bit(name: &str) -> Option<u8> {
    match name.to_ascii_lowercase().as_str() {
        "shift" => Some(0x02),
        "ctrl" | "control" => Some(0x01),
        "alt" => Some(0x04),
        "win" | "gui" => Some(0x08),
        _ => MOD_NAMES.iter().position(|m| m.eq_ignore_ascii_case(name)).map(|b| 1u8 << b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(key_name(0x1A), "W");
        assert_eq!(key_name(0x1E), "1");
        assert_eq!(key_name(0x27), "0");
        assert_eq!(key_name(0x3A), "F1");
        assert_eq!(key_name(0x68), "F13");
        assert_eq!(key_name(0x59), "Num1");
        assert_eq!(key_name(0x62), "Num0");
        assert_eq!(modifier_names(0b0000_0011), vec!["LCtrl", "LShift"]);
    }

    #[test]
    fn every_key_name_is_unique_and_parses_back() {
        for c in 1..=0xFFu8 {
            let n = key_name(c);
            assert!(!n.contains('+'), "{n} contains '+'");
            assert_eq!(key_code(&n), Some(c), "{n}");
        }
    }

    #[test]
    fn modifiers() {
        assert_eq!(modifier_bit("Shift"), Some(0x02));
        assert_eq!(modifier_bit("rwin"), Some(0x80));
        assert_eq!(modifier_bit("W"), None);
    }
}
