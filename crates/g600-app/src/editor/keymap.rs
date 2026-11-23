//! egui keys -> HID keyboard usage codes.
//!
//! We map the *physical* key (its position on a US keyboard), because that is
//! what the mouse sends: HID codes are positions, and Windows applies the
//! keyboard layout afterwards. So on a Swedish keyboard, capturing the key
//! right of P stores "[" (HID 0x2F), which then types "å" as expected.

use eframe::egui::{self, Key};

pub fn hid_code(key: Key) -> Option<u8> {
    use Key::*;
    let code = match key {
        A => 0x04, B => 0x05, C => 0x06, D => 0x07, E => 0x08, F => 0x09, G => 0x0A,
        H => 0x0B, I => 0x0C, J => 0x0D, K => 0x0E, L => 0x0F, M => 0x10, N => 0x11,
        O => 0x12, P => 0x13, Q => 0x14, R => 0x15, S => 0x16, T => 0x17, U => 0x18,
        V => 0x19, W => 0x1A, X => 0x1B, Y => 0x1C, Z => 0x1D,
        Num1 => 0x1E, Num2 => 0x1F, Num3 => 0x20, Num4 => 0x21, Num5 => 0x22,
        Num6 => 0x23, Num7 => 0x24, Num8 => 0x25, Num9 => 0x26, Num0 => 0x27,
        Enter => 0x28,
        Escape => 0x29,
        Backspace => 0x2A,
        Tab => 0x2B,
        Space => 0x2C,
        Minus => 0x2D,
        Equals | Plus => 0x2E,
        OpenBracket => 0x2F,
        CloseBracket => 0x30,
        Backslash => 0x31,
        Semicolon => 0x33,
        Quote => 0x34,
        Backtick => 0x35,
        Comma => 0x36,
        Period => 0x37,
        Slash => 0x38,
        F1 => 0x3A, F2 => 0x3B, F3 => 0x3C, F4 => 0x3D, F5 => 0x3E, F6 => 0x3F,
        F7 => 0x40, F8 => 0x41, F9 => 0x42, F10 => 0x43, F11 => 0x44, F12 => 0x45,
        F13 => 0x68, F14 => 0x69, F15 => 0x6A, F16 => 0x6B, F17 => 0x6C, F18 => 0x6D,
        F19 => 0x6E, F20 => 0x6F, F21 => 0x70, F22 => 0x71, F23 => 0x72, F24 => 0x73,
        Insert => 0x49,
        Home => 0x4A,
        PageUp => 0x4B,
        Delete => 0x4C,
        End => 0x4D,
        PageDown => 0x4E,
        ArrowRight => 0x4F,
        ArrowLeft => 0x50,
        ArrowDown => 0x51,
        ArrowUp => 0x52,
        _ => return None,
    };
    Some(code)
}

/// HID modifier bits (left-hand keys) for egui's modifier state.
pub fn hid_modifiers(m: egui::Modifiers) -> u8 {
    let mut bits = 0;
    if m.ctrl {
        bits |= 0x01;
    }
    if m.shift {
        bits |= 0x02;
    }
    if m.alt {
        bits |= 0x04;
    }
    if m.mac_cmd {
        bits |= 0x08;
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;
    use g600_core::keys::key_name;

    #[test]
    fn maps_to_expected_names() {
        assert_eq!(key_name(hid_code(Key::W).unwrap()), "W");
        assert_eq!(key_name(hid_code(Key::Num0).unwrap()), "0");
        assert_eq!(key_name(hid_code(Key::F13).unwrap()), "F13");
        assert_eq!(key_name(hid_code(Key::OpenBracket).unwrap()), "[");
        assert_eq!(key_name(hid_code(Key::ArrowUp).unwrap()), "Up");
        assert_eq!(hid_modifiers(egui::Modifiers::CTRL | egui::Modifiers::SHIFT), 0x03);
    }
}
