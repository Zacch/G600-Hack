//! Finding the G600 and reading feature reports. Read-only for now.

use hidapi::{DeviceInfo, HidApi, HidDevice};

use crate::{PRODUCT_ID, PROFILE_REPORT_IDS, VENDOR_ID};

pub use hidapi::HidError;

/// All HID collections the G600 exposes (mouse, keyboard, vendor-defined...).
pub fn collections(api: &HidApi) -> Vec<&DeviceInfo> {
    api.device_list()
        .filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID)
        .collect()
}

pub struct G600 {
    dev: HidDevice,
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
                    let g = G600 { dev, path: path.clone() };
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
}
