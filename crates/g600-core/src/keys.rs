//! HID keyboard usage codes and modifier bits (USB HID Usage Tables, page 0x07).

/// Modifier bitmask as used in HID keyboard reports.
pub const MOD_NAMES: [&str; 8] = [
    "LCtrl", "LShift", "LAlt", "LWin", "RCtrl", "RShift", "RAlt", "RWin",
];

pub fn modifier_names(mask: u8) -> Vec<&'static str> {
    (0..8).filter(|b| mask & (1 << b) != 0).map(|b| MOD_NAMES[b]).collect()
}

/// Human-readable name of a HID keyboard usage code (US layout labels).
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
        0x54 => "Num/",
        0x55 => "Num*",
        0x56 => "Num-",
        0x57 => "Num+",
        0x58 => "NumEnter",
        0x59..=0x61 => return format!("Num{}", code - 0x58),
        0x62 => "Num0",
        0x63 => "Num.",
        0x64 => "NonUS\\",
        0x68..=0x73 => return format!("F{}", code - 0x68 + 13),
        _ => return format!("Key(0x{code:02X})"),
    };
    s.to_string()
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
}

/// Parse a combo like "Shift+W", "LCtrl+LAlt+F5" or "Ctrl" into
/// (modifier mask, key code). "Shift"/"Ctrl"/"Alt"/"Win" mean the left key.
pub fn parse_combo(s: &str) -> Result<(u8, u8), String> {
    let mut mods = 0u8;
    let mut key = 0u8;
    for part in s.split('+').map(str::trim) {
        let alias = match part.to_ascii_lowercase().as_str() {
            "shift" => Some(0x02),
            "ctrl" | "control" => Some(0x01),
            "alt" => Some(0x04),
            "win" | "gui" => Some(0x08),
            _ => None,
        };
        if let Some(bit) = alias.or_else(|| {
            MOD_NAMES.iter().position(|m| m.eq_ignore_ascii_case(part)).map(|b| 1u8 << b)
        }) {
            mods |= bit;
            continue;
        }
        if key != 0 {
            return Err(format!("more than one non-modifier key in '{s}'"));
        }
        key = (1..=0xFFu8)
            .find(|&c| key_name(c).eq_ignore_ascii_case(part))
            .ok_or_else(|| format!("unknown key '{part}'"))?;
    }
    if mods == 0 && key == 0 {
        return Err("empty combo".into());
    }
    Ok((mods, key))
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn combos() {
        assert_eq!(parse_combo("Shift+W"), Ok((0x02, 0x1A)));
        assert_eq!(parse_combo("lctrl+lalt+f5"), Ok((0x05, 0x3E)));
        assert_eq!(parse_combo("Space"), Ok((0, 0x2C)));
        assert!(parse_combo("W+Q").is_err());
        assert!(parse_combo("Foo").is_err());
    }
}
