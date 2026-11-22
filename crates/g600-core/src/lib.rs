//! Protocol layer for the Logitech G600 (VID 046D / PID C24A).
//!
//! Onboard profiles live in HID feature reports 0xF3, 0xF4 and 0xF5.
//! Layout follows the community documentation in libratbag's
//! `driver-logitech-g600.c`; verify against real dumps before writing.

pub mod device;
pub mod keys;
pub mod profile;

pub const VENDOR_ID: u16 = 0x046D;
pub const PRODUCT_ID: u16 = 0xC24A;
pub const REPORT_ID_ACTIVE: u8 = 0xF0;
pub const PROFILE_REPORT_IDS: [u8; 3] = [0xF3, 0xF4, 0xF5];
