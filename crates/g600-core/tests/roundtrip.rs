//! Round-trip and content tests against real reports read from a G600.
//!
//! Fixtures (tests/fixtures):
//! - original_F3/F4/F5.bin   factory-ish profiles as first dumped
//! - F3_g9_shift_w.bin       original F3 with G9 set to Shift+W (verified on hardware)
//! - corrupted_shifted_F4.bin  F4 after an unsafe write (byte 3 duplicated at offset 4)

use g600_core::{
    action::Action,
    profile::{button_index, Profile},
};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

const ALL: [&str; 5] = [
    "original_F3.bin",
    "original_F4.bin",
    "original_F5.bin",
    "F3_g9_shift_w.bin",
    "corrupted_shifted_F4.bin",
];

#[test]
fn encode_decode_is_byte_exact() {
    for name in ALL {
        let raw = fixture(name);
        let p = Profile::decode(&raw).unwrap();
        assert_eq!(p.encode().unwrap().as_slice(), raw.as_slice(), "{name}");
    }
}

#[test]
fn button_text_round_trips() {
    for name in ALL {
        let p = Profile::decode(&fixture(name)).unwrap();
        for a in p.buttons.iter().chain(&p.shift_buttons) {
            assert_eq!(a.to_string().parse::<Action>().as_ref(), Ok(a), "{name}: {a}");
        }
    }
}

#[test]
fn decodes_known_content() {
    let g9 = button_index("G9").unwrap();
    let f3 = Profile::decode(&fixture("original_F3.bin")).unwrap();
    assert_eq!(f3.buttons[0], Action::Mouse(1));
    assert_eq!(f3.buttons[5], Action::GShift);
    assert_eq!(f3.buttons[g9].to_string(), "1");
    assert_eq!(f3.shift_buttons[g9].to_string(), "LCtrl+1");
    assert_eq!(f3.buttons[6].to_string(), "LShift+B"); // G7
    assert_eq!(f3.dpi, [1200, 0, 0, 0]);
    assert_eq!(f3.report_rate_hz(), 1000);

    let f4 = Profile::decode(&fixture("original_F4.bin")).unwrap();
    assert_eq!(f4.buttons[g9].to_string(), "Num1");
    assert_eq!(f4.buttons[19].to_string(), "NumPlus"); // G20

    let f5 = Profile::decode(&fixture("original_F5.bin")).unwrap();
    assert_eq!(f5.dpi, [400, 1200, 2000, 3200]);
    assert_eq!(f5.dpi_default, 2);
    assert_eq!(f5.dpi_shift, 400);
    assert_eq!(f5.buttons[5], Action::DpiShift);
    assert_eq!(f5.buttons[15], Action::Unknown([0x16, 0, 0])); // G16, meaning unknown
    assert!(f5.shift_buttons.iter().all(|a| *a == Action::Disabled));
}

#[test]
fn shift_w_edit_reproduces_hardware_dump() {
    let mut p = Profile::decode(&fixture("original_F3.bin")).unwrap();
    *p.button_mut(button_index("G9").unwrap(), false) = "Shift+W".parse().unwrap();
    assert_eq!(p.encode().unwrap().as_slice(), fixture("F3_g9_shift_w.bin").as_slice());
}
