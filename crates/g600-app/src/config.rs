//! profiles.toml: named profiles, which apps use them, and the fallback.
//!
//! Every profile starts from the factory layout (`Profile::factory()`) and
//! only lists what differs. Example:
//!
//! ```toml
//! default = "desktop"
//!
//! [profiles.desktop]
//!
//! [profiles.skyrim]
//! apps = ["SkyrimSE.exe"]
//! led = "#FF4000"
//! dpi = [800, 1600]
//! default_dpi = 1
//! buttons = { G9 = "Shift+W", G10 = "LCtrl", G11 = "Space" }
//! gshift = { G9 = "LAlt+F4" }
//! ```
//!
//! `ConfigFile` is the file as written (editable, saveable); `Config` is the
//! validated, compiled form the switcher uses.

use std::{collections::BTreeMap, path::Path};

use g600_core::{
    action::Action,
    profile::{button_index, Led, Profile, Rgb, BUTTON_NAMES},
};
use serde::{Deserialize, Serialize};

pub const DEFAULT_DEBOUNCE_MS: u64 = 400;

/// profiles.toml as written.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    pub default: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debounce_ms: Option<u64>,
    #[serde(default)]
    pub profiles: BTreeMap<String, ProfileEntry>,
}

/// One `[profiles.<name>]` table. Unset fields keep the factory values.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileEntry {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub led: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gshift_led: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi: Option<Vec<u16>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_dpi: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi_shift: Option<u16>,
    /// Button name -> action text, normal layer.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub buttons: BTreeMap<String, String>,
    /// Button name -> action text, G-Shift layer.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub gshift: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct NamedProfile {
    pub name: String,
    /// Lower-cased exe file names, e.g. "skyrimse.exe".
    pub apps: Vec<String>,
    pub profile: Profile,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub profiles: Vec<NamedProfile>,
    pub default: usize,
    pub debounce_ms: u64,
}

impl ProfileEntry {
    /// The effective profile (factory + this entry) and any problems found.
    /// The profile is usable for display even when there are problems.
    pub fn build(&self) -> (Profile, Vec<String>) {
        let mut errors = Vec::new();
        let mut err = |m: String| errors.push(m);
        let mut p = Profile::factory();

        if let Some(c) = &self.led {
            match parse_rgb(c) {
                Ok(rgb) => p.led = Led { color: rgb, effect: 0, duration: 0 },
                Err(e) => err(format!("led: {e}")),
            }
        }
        if let Some(c) = &self.gshift_led {
            match parse_rgb(c) {
                Ok(rgb) => p.shift_color = rgb,
                Err(e) => err(format!("gshift_led: {e}")),
            }
        }
        if let Some(dpis) = &self.dpi {
            if dpis.is_empty() || dpis.len() > 4 {
                err("dpi: give 1 to 4 values".into());
            } else {
                for slot in 1..=4 {
                    let d = dpis.get(slot - 1).copied().unwrap_or(0);
                    if let Err(e) = p.set_dpi(slot, d) {
                        err(format!("dpi: {e}"));
                    }
                }
                p.dpi_default = 1;
            }
        }
        if let Some(d) = self.default_dpi {
            let n = p.dpi.iter().filter(|&&x| x != 0).count() as u8;
            if d == 0 || d > n {
                err(format!("default_dpi {d}: must be 1..={n}"));
            } else {
                p.dpi_default = d;
            }
        }
        if let Some(d) = self.dpi_shift {
            if d != 0 && (d % 50 != 0 || !(200..=8200).contains(&d)) {
                err(format!("dpi_shift {d}: must be 200-8200 in steps of 50"));
            } else {
                p.dpi_shift = d;
            }
        }
        for (layer, map, shift) in [("buttons", &self.buttons, false), ("gshift", &self.gshift, true)] {
            for (button, action) in map {
                let Some(i) = button_index(button) else {
                    err(format!("{layer}: unknown button '{button}' (known: {})", BUTTON_NAMES.join(", ")));
                    continue;
                };
                match action.parse::<Action>() {
                    Ok(a) => *p.button_mut(i, shift) = a,
                    Err(e) => err(format!("{layer}.{button}: {e}")),
                }
            }
        }
        if !p.has_left_click() {
            err("no button is Mouse1 (left click); refusing a profile you could not click with".into());
        }
        if let Err(e) = p.encode() {
            err(e.to_string());
        }
        (p, errors)
    }
}

impl ConfigFile {
    pub fn parse(text: &str) -> Result<ConfigFile, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// TOML text for saving, with a short header explaining the format.
    pub fn to_toml(&self) -> String {
        let body = toml::to_string_pretty(self).expect("config serializes");
        format!("{SAVED_HEADER}{body}")
    }

    /// Validate everything. Reports every problem found, not just the first.
    pub fn compile(&self) -> Result<Config, String> {
        let mut errors = Vec::new();
        let mut profiles = Vec::new();
        let mut owner: BTreeMap<String, String> = BTreeMap::new();

        for (name, entry) in &self.profiles {
            let (p, errs) = entry.build();
            errors.extend(errs.into_iter().map(|e| format!("[profiles.{name}] {e}")));
            let mut apps = Vec::new();
            for app in &entry.apps {
                let key = app.trim().to_ascii_lowercase();
                if let Some(other) = owner.insert(key.clone(), name.clone()) {
                    errors.push(format!("[profiles.{name}] app '{app}' is already used by profile '{other}'"));
                }
                apps.push(key);
            }
            profiles.push(NamedProfile { name: name.clone(), apps, profile: p });
        }

        let default = profiles.iter().position(|p| p.name == self.default);
        if default.is_none() {
            errors.push(format!("default = \"{}\" but there is no [profiles.{}]", self.default, self.default));
        }
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
        Ok(Config {
            profiles,
            default: default.unwrap(),
            debounce_ms: self.debounce_ms.unwrap_or(DEFAULT_DEBOUNCE_MS),
        })
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Config::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Config, String> {
        ConfigFile::parse(text)?.compile()
    }

    /// The profile for a foreground exe (file name, any case), or the default.
    pub fn profile_for(&self, exe: &str) -> &NamedProfile {
        let exe = exe.to_ascii_lowercase();
        self.profiles
            .iter()
            .find(|p| p.apps.iter().any(|a| *a == exe))
            .unwrap_or(&self.profiles[self.default])
    }
}

pub fn parse_rgb(s: &str) -> Result<Rgb, String> {
    let h = s.trim().strip_prefix('#').unwrap_or(s.trim());
    if h.len() != 6 {
        return Err(format!("'{s}' is not #RRGGBB"));
    }
    let v = u32::from_str_radix(h, 16).map_err(|_| format!("'{s}' is not #RRGGBB"))?;
    Ok(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

pub fn format_rgb(c: Rgb) -> String {
    format!("#{:02X}{:02X}{:02X}", c.0, c.1, c.2)
}

/// Header written when the editor saves (comments in the old file are not kept).
const SAVED_HEADER: &str = "\
# G600 switcher profiles (saved by the G600 profile editor).
# Profiles start from the factory layout and list only what differs.
# Actions: \"Shift+W\", \"LCtrl+F5\", \"Space\", \"Mouse1\"..\"Mouse5\", \"DpiUp\", \"DpiDown\",
#   \"DpiCycle\", \"ProfileCycle\", \"DpiShift\", \"GShift\", \"Disabled\".
# Buttons: Left, Right, Middle, Tilt-L, Tilt-R, Ring, G7 .. G20.

";

/// Written on first start. The Notepad profile is a demo of switching.
pub const STARTER_CONFIG: &str = r##"# G600 switcher profiles. Edit, save, then choose "Reload config" in the tray menu.
#
# Every profile starts from the factory layout and lists only what differs:
#   Left/Right/Middle = Mouse1/2/3, Tilt-L/Tilt-R = Mouse4/5, Ring = GShift,
#   G7 = LShift+B, G8 = ProfileCycle, G9..G20 = 1 2 3 4 5 6 7 8 9 0 - =
#   G-Shift layer: same, but G9..G20 = LCtrl+1 ... LCtrl+=
#
# Actions: key combos like "Shift+W", "LCtrl+LAlt+F5", "Space", "NumPlus";
#   "Mouse1".."Mouse5", "DpiUp", "DpiDown", "DpiCycle", "ProfileCycle",
#   "DpiShift", "GShift", "Disabled".
# Buttons: Left, Right, Middle, Tilt-L, Tilt-R, Ring, G7 .. G20.
# apps: exe file names (see Task Manager > Details), case doesn't matter.

# Profile used when the foreground app has no profile of its own.
default = "desktop"

[profiles.desktop]

# Demo: in Notepad, G9 types Shift+W and the LED turns green.
[profiles.notepad-demo]
apps = ["notepad.exe"]
led = "#00FF00"
buttons = { G9 = "Shift+W" }

# Example game profile (remove the # signs to use it):
# [profiles.skyrim]
# apps = ["SkyrimSE.exe"]
# led = "#FF4000"
# dpi = [800, 1600]
# default_dpi = 1
# buttons = { G9 = "Shift+W", G10 = "LCtrl", G11 = "Space" }
# gshift = { G9 = "LAlt+F4" }
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_config_is_valid() {
        let c = Config::parse(STARTER_CONFIG).unwrap();
        assert_eq!(c.profile_for("explorer.exe").name, "desktop");
        let n = c.profile_for("NOTEPAD.EXE");
        assert_eq!(n.name, "notepad-demo");
        assert_eq!(n.profile.buttons[8].to_string(), "LShift+W");
        assert_eq!(n.profile.led.color, Rgb(0, 255, 0));
        // The desktop profile is exactly the factory layout.
        assert_eq!(c.profiles[c.default].profile, Profile::factory());
    }

    #[test]
    fn full_profile() {
        let c = Config::parse(
            r##"
            default = "d"
            [profiles.d]
            [profiles.g]
            apps = ["Game.exe"]
            dpi = [800, 1600]
            default_dpi = 2
            dpi_shift = 400
            gshift_led = "#010203"
            buttons = { G10 = "LCtrl", Tilt-L = "DpiCycle" }
            gshift = { G20 = "Disabled" }
            "##,
        )
        .unwrap();
        let g = &c.profile_for("game.exe").profile;
        assert_eq!(g.dpi, [800, 1600, 0, 0]);
        assert_eq!(g.dpi_default, 2);
        assert_eq!(g.dpi_shift, 400);
        assert_eq!(g.shift_color, Rgb(1, 2, 3));
        assert_eq!(g.buttons[9].to_string(), "LCtrl");
        assert_eq!(g.buttons[3], Action::DpiCycle);
        assert_eq!(g.shift_buttons[19], Action::Disabled);
    }

    #[test]
    fn reports_all_errors() {
        let e = Config::parse(
            r##"
            default = "missing"
            [profiles.a]
            apps = ["x.exe"]
            led = "green"
            dpi = [123]
            buttons = { G99 = "W", G9 = "Foo", Left = "Mouse2" }
            [profiles.b]
            apps = ["X.EXE"]
            "##,
        )
        .unwrap_err();
        for needle in ["led:", "dpi:", "G99", "G9: 'Foo'", "Mouse1", "already used", "no [profiles.missing]"] {
            assert!(e.contains(needle), "missing '{needle}' in:\n{e}");
        }
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!(Config::parse("default = \"d\"\n[profiles.d]\nbutons = {}\n").is_err());
    }

    #[test]
    fn save_and_reload_gives_same_config() {
        let f = ConfigFile::parse(STARTER_CONFIG).unwrap();
        let text = f.to_toml();
        let again = ConfigFile::parse(&text).unwrap();
        assert_eq!(f, again, "saved text:\n{text}");
        assert!(again.compile().is_ok());
    }
}
