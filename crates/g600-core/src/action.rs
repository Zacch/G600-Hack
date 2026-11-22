//! What a G600 button does: one 3-byte entry `{code, modifier, key}`.
//!
//! Decoding is lossless: any entry we don't fully understand becomes
//! `Action::Unknown` and encodes back to the same bytes.
//!
//! Text form (used by the CLI and, later, profile files):
//! `Disabled`, `Mouse1`..`Mouse5`, `DpiUp`, `DpiDown`, `DpiCycle`,
//! `ProfileCycle`, `DpiShift`, `GShift`, key combos like `Shift+W` or
//! `LCtrl+LAlt+F5`, and `Raw(16 00 00)` for anything else.

use std::{fmt, str::FromStr};

use crate::keys::{key_code, key_name, modifier_bit, modifier_names};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Disabled,
    /// Mouse button 1-5 (left, right, middle, back, forward).
    Mouse(u8),
    /// Keyboard key with modifier bitmask. `key` 0 = modifiers only.
    Key { modifiers: u8, key: u8 },
    DpiUp,
    DpiDown,
    DpiCycle,
    ProfileCycle,
    /// Use the profile's "DPI shift" value while held.
    DpiShift,
    /// Switch to the G-Shift button layer while held.
    GShift,
    /// Anything else, kept byte-exact.
    Unknown([u8; 3]),
}

const SIMPLE: [(u8, Action, &str); 6] = [
    (0x11, Action::DpiUp, "DpiUp"),
    (0x12, Action::DpiDown, "DpiDown"),
    (0x13, Action::DpiCycle, "DpiCycle"),
    (0x14, Action::ProfileCycle, "ProfileCycle"),
    (0x15, Action::DpiShift, "DpiShift"),
    (0x17, Action::GShift, "GShift"),
];

impl Action {
    pub fn decode(b: [u8; 3]) -> Action {
        let [code, modifiers, key] = b;
        match code {
            0x00 if modifiers == 0 && key == 0 => Action::Disabled,
            0x00 => Action::Key { modifiers, key },
            0x01..=0x05 if modifiers == 0 && key == 0 => Action::Mouse(code),
            _ if modifiers == 0 && key == 0 => SIMPLE
                .iter()
                .find(|(c, _, _)| *c == code)
                .map(|(_, a, _)| *a)
                .unwrap_or(Action::Unknown(b)),
            _ => Action::Unknown(b),
        }
    }

    pub fn encode(&self) -> [u8; 3] {
        match *self {
            Action::Disabled => [0, 0, 0],
            Action::Mouse(n) => [n, 0, 0],
            Action::Key { modifiers, key } => [0, modifiers, key],
            Action::Unknown(b) => b,
            simple => {
                let code = SIMPLE.iter().find(|(_, a, _)| *a == simple).map(|(c, _, _)| *c);
                [code.expect("every simple action is in SIMPLE"), 0, 0]
            }
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Action::Disabled => f.write_str("Disabled"),
            Action::Mouse(n) => write!(f, "Mouse{n}"),
            Action::Key { modifiers, key } => {
                let mut parts: Vec<String> =
                    modifier_names(modifiers).into_iter().map(String::from).collect();
                if key != 0 {
                    parts.push(key_name(key));
                }
                f.write_str(&parts.join("+"))
            }
            Action::Unknown([a, b, c]) => write!(f, "Raw({a:02X} {b:02X} {c:02X})"),
            simple => {
                let name = SIMPLE.iter().find(|(_, a, _)| *a == simple).map(|(_, _, n)| *n);
                f.write_str(name.expect("every simple action is in SIMPLE"))
            }
        }
    }
}

impl FromStr for Action {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("disabled") {
            return Ok(Action::Disabled);
        }
        if let Some((_, a, _)) = SIMPLE.iter().find(|(_, _, n)| n.eq_ignore_ascii_case(s)) {
            return Ok(*a);
        }
        if let Some(n) = strip_prefix_ci(s, "mouse") {
            return match n.parse::<u8>() {
                Ok(n @ 1..=5) => Ok(Action::Mouse(n)),
                _ => Err(format!("'{s}': mouse buttons are Mouse1..Mouse5")),
            };
        }
        if let Some(inner) = strip_prefix_ci(s, "raw(").and_then(|r| r.strip_suffix(')')) {
            let bytes: Vec<u8> = inner
                .split_whitespace()
                .map(|h| u8::from_str_radix(h, 16))
                .collect::<Result<_, _>>()
                .map_err(|_| format!("'{s}': expected Raw(XX XX XX) in hex"))?;
            let b: [u8; 3] = bytes.try_into().map_err(|_| format!("'{s}': Raw needs 3 bytes"))?;
            return Ok(Action::decode(b));
        }
        parse_combo(s)
    }
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &s[prefix.len()..])
}

fn parse_combo(s: &str) -> Result<Action, String> {
    let mut modifiers = 0u8;
    let mut key = 0u8;
    for part in s.split('+').map(str::trim) {
        if let Some(bit) = modifier_bit(part) {
            modifiers |= bit;
        } else if key != 0 {
            return Err(format!("'{s}': more than one non-modifier key"));
        } else {
            key = key_code(part).ok_or_else(|| format!("'{s}': unknown key '{part}'"))?;
        }
    }
    if modifiers == 0 && key == 0 {
        return Err(format!("'{s}': empty combo"));
    }
    Ok(Action::Key { modifiers, key })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Action {
        s.parse().unwrap()
    }

    #[test]
    fn parses_text() {
        assert_eq!(p("Shift+W"), Action::Key { modifiers: 0x02, key: 0x1A });
        assert_eq!(p("lctrl+lalt+f5"), Action::Key { modifiers: 0x05, key: 0x3E });
        assert_eq!(p("Space"), Action::Key { modifiers: 0, key: 0x2C });
        assert_eq!(p("LCtrl+NumPlus"), Action::Key { modifiers: 0x01, key: 0x57 });
        assert_eq!(p("mouse4"), Action::Mouse(4));
        assert_eq!(p("GShift"), Action::GShift);
        assert_eq!(p("Raw(16 00 00)"), Action::Unknown([0x16, 0, 0]));
        assert_eq!(p("Raw(13 00 00)"), Action::DpiCycle); // normalised
        for bad in ["W+Q", "Foo", "Mouse9", "Raw(1 2)", ""] {
            assert!(bad.parse::<Action>().is_err(), "{bad}");
        }
    }

    #[test]
    fn decode_encode_is_lossless() {
        for code in 0..=0xFFu8 {
            for (m, k) in [(0, 0), (0, 0x1A), (0x02, 0), (0x02, 0x1A), (0xFF, 0xFF)] {
                let b = [code, m, k];
                assert_eq!(Action::decode(b).encode(), b, "{b:02X?}");
            }
        }
    }

    #[test]
    fn text_round_trips() {
        for code in 0..=0xFFu8 {
            for (m, k) in [(0, 0), (0, 0x1A), (0x22, 0x57), (0xFF, 0xFF)] {
                let a = Action::decode([code, m, k]);
                assert_eq!(a.to_string().parse::<Action>(), Ok(a), "{a}");
            }
        }
    }
}
