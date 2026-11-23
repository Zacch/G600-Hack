//! G600 profile editor: a GUI for %APPDATA%\g600cfg\profiles.toml.
//!
//! It only edits the file; the tray switcher picks up saved changes at the
//! next app switch (or via "Reload profiles"). Nothing here talks to the mouse.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod diagram;
mod keymap;

use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};

use eframe::egui::{self, Color32, RichText};
use g600_app::{
    apps::open_window_exes,
    config::{format_rgb, parse_rgb, ConfigFile, ProfileEntry, STARTER_CONFIG},
    config_dir,
};
use g600_core::{
    action::Action,
    keys::{key_code, key_name, MOD_NAMES},
    profile::{button_index, Profile, Rgb, BUTTON_NAMES},
};

fn main() -> eframe::Result {
    let path = config_dir().map(|d| d.join("profiles.toml")).unwrap_or_else(|| PathBuf::from("profiles.toml"));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("G600 profiles")
            .with_inner_size([1280.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("G600 profiles", options, Box::new(move |_cc| Ok(Box::new(Editor::load(path)))))
}

/// What a button is set to, as chosen in the button editor.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Default,
    Key,
    Mouse,
    DpiUp,
    DpiDown,
    DpiCycle,
    DpiShift,
    ProfileCycle,
    GShift,
    Disabled,
}

impl Kind {
    const ALL: [(Kind, &'static str); 10] = [
        (Kind::Default, "Factory default"),
        (Kind::Key, "Key / combo"),
        (Kind::Mouse, "Mouse button"),
        (Kind::DpiUp, "DPI up"),
        (Kind::DpiDown, "DPI down"),
        (Kind::DpiCycle, "DPI cycle"),
        (Kind::DpiShift, "DPI shift (hold)"),
        (Kind::ProfileCycle, "Profile cycle"),
        (Kind::GShift, "G-Shift (hold)"),
        (Kind::Disabled, "Disabled"),
    ];

    fn of(a: &Action) -> Kind {
        match a {
            Action::Key { .. } => Kind::Key,
            Action::Mouse(_) => Kind::Mouse,
            Action::DpiUp => Kind::DpiUp,
            Action::DpiDown => Kind::DpiDown,
            Action::DpiCycle => Kind::DpiCycle,
            Action::DpiShift => Kind::DpiShift,
            Action::ProfileCycle => Kind::ProfileCycle,
            Action::GShift => Kind::GShift,
            Action::Disabled => Kind::Disabled,
            Action::Unknown(_) => Kind::Default,
        }
    }
}

struct Editor {
    path: PathBuf,
    file: ConfigFile,
    saved_text: String,
    load_error: Option<String>,
    selected: String,
    button: Option<usize>,
    shift_layer: bool,
    capturing: bool,
    draft: Option<Action>,
    key_text: String,
    new_app: String,
    rename: String,
    open_apps: Vec<String>,
    open_apps_at: Option<Instant>,
    status: String,
    confirm_close: bool,
    allow_close: bool,
}

impl Editor {
    fn load(path: PathBuf) -> Editor {
        let text = std::fs::read_to_string(&path).unwrap_or_else(|_| STARTER_CONFIG.to_string());
        let (file, load_error) = match ConfigFile::parse(&text) {
            Ok(f) => (f, None),
            Err(e) => (ConfigFile::parse(STARTER_CONFIG).expect("starter config parses"), Some(e)),
        };
        let selected = file.default.clone();
        let saved_text = file.to_toml();
        Editor {
            path,
            rename: selected.clone(),
            selected,
            saved_text,
            file,
            load_error,
            button: Some(8),
            shift_layer: false,
            capturing: false,
            draft: None,
            key_text: String::new(),
            new_app: String::new(),
            open_apps: Vec::new(),
            open_apps_at: None,
            status: String::new(),
            confirm_close: false,
            allow_close: false,
        }
    }

    fn dirty(&self) -> bool {
        self.file.to_toml() != self.saved_text
    }

    fn save(&mut self) {
        if let Err(e) = self.file.compile() {
            self.status = format!("Not saved, fix these first:\n{e}");
            return;
        }
        let text = self.file.to_toml();
        let tmp = self.path.with_extension("toml.tmp");
        let result = std::fs::create_dir_all(self.path.parent().unwrap_or(&self.path))
            .and_then(|_| std::fs::write(&tmp, &text))
            .and_then(|_| std::fs::rename(&tmp, &self.path));
        match result {
            Ok(()) => {
                self.saved_text = text;
                self.status = "Saved. The switcher applies it at the next app switch.".into();
            }
            Err(e) => self.status = format!("Saving {} failed: {e}", self.path.display()),
        }
    }

    fn entry(&mut self) -> &mut ProfileEntry {
        self.file.profiles.entry(self.selected.clone()).or_default()
    }

    fn select_profile(&mut self, name: String) {
        self.rename = name.clone();
        self.selected = name;
        self.capturing = false;
        self.draft = None;
        self.key_text.clear();
    }
}

impl eframe::App for Editor {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_close(&ctx);
        if self.capturing {
            self.capture_key(&ctx);
        }

        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        if let Some(e) = self.load_error.clone() {
            egui::CentralPanel::default().show(ui, |ui| self.load_error_view(ui, &e));
            return;
        }
        egui::Panel::left("profiles").resizable(false).exact_size(210.0).show(ui, |ui| self.profile_list(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.profile_view(ui));
        });
        self.close_dialog(&ctx);
    }
}

impl Editor {
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.heading("G600 profiles");
            ui.add_space(16.0);
            let dirty = self.dirty();
            if ui.add_enabled(dirty, egui::Button::new("💾 Save")).clicked() {
                self.save();
            }
            if ui.add_enabled(dirty, egui::Button::new("Revert")).clicked() {
                *self = Editor::load(self.path.clone());
                self.status = "Reverted to the saved file.".into();
            }
            if dirty {
                ui.label(RichText::new("● unsaved changes").color(Color32::from_rgb(0xE0, 0xA0, 0x30)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(self.path.display().to_string()).small().weak());
            });
        });
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        ui.add_space(4.0);
    }

    fn load_error_view(&mut self, ui: &mut egui::Ui, e: &str) {
        ui.heading("profiles.toml has an error");
        ui.label("Fix it in a text editor, or start over from the starter profiles.");
        ui.add_space(8.0);
        ui.code(e);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Open in Notepad").clicked() {
                let _ = std::process::Command::new("notepad.exe").arg(&self.path).spawn();
            }
            if ui.button("Reload").clicked() {
                *self = Editor::load(self.path.clone());
            }
            if ui.button("Discard file, use starter profiles").clicked() {
                self.load_error = None;
                self.saved_text.clear(); // force "unsaved"
            }
        });
    }

    fn profile_list(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.strong("Profiles");
        ui.add_space(4.0);
        let names: Vec<String> = self.file.profiles.keys().cloned().collect();
        for name in names {
            let is_default = name == self.file.default;
            let label = if is_default { format!("⭐ {name}") } else { name.clone() };
            let apps = self.file.profiles[&name].apps.len();
            let resp = ui.selectable_label(self.selected == name, label).on_hover_text(if is_default {
                "Default: used for every app without its own profile".to_string()
            } else {
                format!("{apps} app(s)")
            });
            if resp.clicked() {
                self.select_profile(name);
            }
        }
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("➕ New").clicked() {
                let name = unique_name(&self.file.profiles, "new-profile");
                self.file.profiles.insert(name.clone(), ProfileEntry::default());
                self.select_profile(name);
            }
            if ui.button("Duplicate").clicked() {
                let mut copy = self.file.profiles.get(&self.selected).cloned().unwrap_or_default();
                copy.apps.clear(); // an app can only belong to one profile
                let name = unique_name(&self.file.profiles, &format!("{}-copy", self.selected));
                self.file.profiles.insert(name.clone(), copy);
                self.select_profile(name);
            }
            let can_delete = self.selected != self.file.default;
            if ui
                .add_enabled(can_delete, egui::Button::new("Delete"))
                .on_disabled_hover_text("The default profile can't be deleted")
                .clicked()
            {
                self.file.profiles.remove(&self.selected);
                self.select_profile(self.file.default.clone());
            }
        });
        ui.add_space(12.0);
        ui.label(RichText::new("⭐ = default profile, used when the foreground app has no profile of its own.").small().weak());
    }

    fn profile_view(&mut self, ui: &mut egui::Ui) {
        if !self.file.profiles.contains_key(&self.selected) {
            self.select_profile(self.file.default.clone());
        }
        let (effective, problems) = self.entry().build();

        // Name and default.
        ui.horizontal(|ui| {
            ui.label("Name");
            let r = ui.text_edit_singleline(&mut self.rename);
            let new = self.rename.trim().to_string();
            let valid = !new.is_empty() && (new == self.selected || !self.file.profiles.contains_key(&new));
            if new != self.selected && ui.add_enabled(valid, egui::Button::new("Rename")).clicked()
                || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && valid && new != self.selected)
            {
                let entry = self.file.profiles.remove(&self.selected).unwrap_or_default();
                self.file.profiles.insert(new.clone(), entry);
                if self.file.default == self.selected {
                    self.file.default = new.clone();
                }
                self.select_profile(new);
            }
            ui.add_space(16.0);
            if self.file.default == self.selected {
                ui.label("⭐ Default profile");
            } else if ui.button("Make default").clicked() {
                self.file.default = self.selected.clone();
            }
        });
        ui.separator();

        self.apps_section(ui);
        ui.separator();
        self.led_dpi_section(ui, &effective);
        ui.separator();

        // Layer switch and diagrams.
        ui.horizontal(|ui| {
            ui.strong("Buttons");
            ui.add_space(12.0);
            let was = self.shift_layer;
            ui.selectable_value(&mut self.shift_layer, false, "Normal");
            ui.selectable_value(&mut self.shift_layer, true, "G-Shift layer (while Ring/G-Shift is held)");
            if was != self.shift_layer {
                self.draft = None;
                self.capturing = false;
                self.key_text.clear();
            }
        });
        let factory = Profile::factory();
        let make_view = |shift: bool| {
            let (actions, base) =
                if shift { (&effective.shift_buttons, &factory.shift_buttons) } else { (&effective.buttons, &factory.buttons) };
            diagram::ButtonView {
                actions,
                changed: std::array::from_fn(|i| actions[i] != base[i]),
                selected: if shift == self.shift_layer { self.button } else { None },
                shift,
                capturing: self.capturing,
            }
        };
        let (normal, gshift) = (make_view(false), make_view(true));
        let top = if self.shift_layer { &gshift } else { &normal };
        ui.label(
            RichText::new("Click a button to edit it. Right-click a button, then press a key (with Ctrl/Shift/Alt) to set it quickly.")
                .small()
                .weak(),
        );
        // (click, layer) of a click in any of the views.
        let mut clicked: Option<(diagram::Click, bool)> = None;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                let layer = if self.shift_layer { "G-Shift layer" } else { "normal layer" };
                ui.label(RichText::new(format!("Top ({layer})")).weak());
                clicked = clicked.or(diagram::top_view(ui, top).map(|c| (c, self.shift_layer)));
            });
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Thumb buttons").weak());
                clicked = clicked.or(diagram::side_view(ui, &normal).map(|c| (c, false)));
                ui.add_space(6.0);
                ui.label(RichText::new("Thumb buttons, G-Shift layer").weak());
                clicked = clicked.or(diagram::side_view(ui, &gshift).map(|c| (c, true)));
            });
        });
        if let Some((c, shift)) = clicked {
            let i = c.button;
            self.button = Some(i);
            self.shift_layer = shift;
            self.key_text.clear();
            self.capturing = c.capture;
            self.draft = None;
            if c.capture {
                // Open the Key editor in capture mode; the next key press is stored.
                let current = if shift { effective.shift_buttons[i] } else { effective.buttons[i] };
                self.draft = Some(match current {
                    Action::Key { .. } => current,
                    _ => Action::Key { modifiers: 0, key: 0 },
                });
            }
        }
        ui.separator();
        if let Some(i) = self.button {
            let (current, base) = if self.shift_layer {
                (effective.shift_buttons[i], factory.shift_buttons[i])
            } else {
                (effective.buttons[i], factory.buttons[i])
            };
            self.button_editor(ui, i, current, base);
        }

        if !problems.is_empty() {
            ui.separator();
            for p in &problems {
                ui.colored_label(Color32::from_rgb(0xE0, 0x50, 0x50), format!("⚠ {p}"));
            }
        }
        if let Err(e) = self.file.compile() {
            let other: Vec<&str> = e.lines().filter(|l| !l.starts_with(&format!("[profiles.{}]", self.selected))).collect();
            if !other.is_empty() {
                ui.separator();
                ui.label("Problems in other profiles:");
                for l in other {
                    ui.colored_label(Color32::from_rgb(0xE0, 0x50, 0x50), format!("⚠ {l}"));
                }
            }
        }
    }

    fn apps_section(&mut self, ui: &mut egui::Ui) {
        ui.strong("Apps");
        if self.file.default == self.selected {
            ui.label(RichText::new("The default profile is also used for every app not listed in another profile.").weak());
        }
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            let apps = self.entry().apps.clone();
            if apps.is_empty() {
                ui.label(RichText::new("(no apps)").weak());
            }
            for (k, app) in apps.iter().enumerate() {
                if ui.button(format!("{app}  ✕")).on_hover_text("Remove").clicked() {
                    remove = Some(k);
                }
            }
        });
        if let Some(k) = remove {
            self.entry().apps.remove(k);
        }
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.new_app).hint_text("game.exe").desired_width(180.0));
            let name = self.new_app.trim().to_string();
            if ui.add_enabled(!name.is_empty(), egui::Button::new("Add")).clicked() {
                self.add_app(name);
                self.new_app.clear();
            }
            let stale = self.open_apps_at.is_none_or(|t| t.elapsed() > Duration::from_secs(3));
            let combo = egui::ComboBox::from_id_salt("open-apps").selected_text("Pick an open app…").width(220.0);
            let r = combo.show_ui(ui, |ui| {
                for exe in self.open_apps.clone() {
                    if ui.selectable_label(false, &exe).clicked() {
                        self.add_app(exe);
                    }
                }
            });
            if r.response.clicked() && stale {
                self.open_apps = open_window_exes();
                self.open_apps_at = Some(Instant::now());
            }
        });
    }

    fn add_app(&mut self, exe: String) {
        let key = exe.to_ascii_lowercase();
        // An app belongs to one profile: move it here from any other.
        for entry in self.file.profiles.values_mut() {
            entry.apps.retain(|a| a.to_ascii_lowercase() != key);
        }
        self.entry().apps.push(exe);
    }

    fn led_dpi_section(&mut self, ui: &mut egui::Ui, effective: &Profile) {
        ui.horizontal(|ui| {
            ui.strong("Lighting");
            ui.add_space(8.0);
            color_option(ui, "Custom colour", &mut self.entry().led, effective.led.color);
            ui.add_space(16.0);
            color_option(ui, "G-Shift colour", &mut self.entry().gshift_led, effective.shift_color);
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.strong("DPI");
            ui.add_space(8.0);
            let entry = self.entry();
            let mut custom = entry.dpi.is_some();
            if ui.checkbox(&mut custom, "Custom").changed() {
                entry.dpi = custom.then(|| effective.dpi.iter().copied().filter(|&d| d != 0).collect());
                entry.default_dpi = None;
            }
            if let Some(dpis) = &mut entry.dpi {
                let mut default = entry.default_dpi.unwrap_or(1);
                for k in 0..4 {
                    let mut on = k < dpis.len();
                    // Slots must be contiguous: only the next one can be added,
                    // only the last one removed.
                    let can_toggle = (on && k == dpis.len() - 1 && k > 0) || (!on && k == dpis.len());
                    ui.add_enabled_ui(can_toggle || on, |ui| {
                        if ui.add_enabled(can_toggle, egui::Checkbox::without_text(&mut on)).changed() {
                            if on {
                                dpis.push(*dpis.last().unwrap_or(&800));
                            } else {
                                dpis.pop();
                            }
                        }
                        if k < dpis.len() {
                            ui.add(egui::DragValue::new(&mut dpis[k]).range(200..=8200).speed(50.0).suffix(" dpi"));
                            dpis[k] = (dpis[k] / 50) * 50;
                            ui.radio_value(&mut default, (k + 1) as u8, "").on_hover_text("Start with this DPI");
                        }
                    });
                    ui.add_space(6.0);
                }
                default = default.min(dpis.len() as u8);
                entry.default_dpi = (default != 1).then_some(default);
            } else {
                let d: Vec<String> = effective.dpi.iter().filter(|&&d| d != 0).map(|d| d.to_string()).collect();
                ui.label(RichText::new(format!("factory: {}", d.join(", "))).weak());
            }
        });
        ui.horizontal(|ui| {
            ui.label("DPI while a \"DPI shift\" button is held:");
            let entry = self.entry();
            let mut on = entry.dpi_shift.is_some();
            if ui.checkbox(&mut on, "").changed() {
                entry.dpi_shift = on.then_some(400);
            }
            if let Some(d) = &mut entry.dpi_shift {
                ui.add(egui::DragValue::new(d).range(200..=8200).speed(50.0).suffix(" dpi"));
                *d = (*d / 50) * 50;
            } else {
                ui.label(RichText::new("not set").weak());
            }
        });
    }

    fn button_editor(&mut self, ui: &mut egui::Ui, i: usize, current: Action, factory: Action) {
        let layer = if self.shift_layer { "G-Shift layer" } else { "normal layer" };
        ui.horizontal(|ui| {
            ui.heading(format!("{} — {layer}", BUTTON_NAMES[i]));
            ui.label(RichText::new(format!("factory: {factory}")).weak());
        });
        // A draft keeps the Key/Mouse editor open while the user is working on
        // it, even when the value equals the factory one or is incomplete.
        let shown = self.draft.unwrap_or(current);
        let overridden = current != factory;
        let mut kind = if self.draft.is_some() || overridden { Kind::of(&shown) } else { Kind::Default };
        let before = kind;
        ui.horizontal_wrapped(|ui| {
            for (k, label) in Kind::ALL {
                ui.selectable_value(&mut kind, k, label);
            }
        });
        let mut new = shown;
        if kind != before {
            self.capturing = false;
            self.key_text.clear();
            new = match kind {
                Kind::Default => factory,
                Kind::Key => match current {
                    Action::Key { .. } => current,
                    _ => Action::Key { modifiers: 0, key: 0 },
                },
                Kind::Mouse => match current {
                    Action::Mouse(_) => current,
                    _ => Action::Mouse(1),
                },
                Kind::DpiUp => Action::DpiUp,
                Kind::DpiDown => Action::DpiDown,
                Kind::DpiCycle => Action::DpiCycle,
                Kind::DpiShift => Action::DpiShift,
                Kind::ProfileCycle => Action::ProfileCycle,
                Kind::GShift => Action::GShift,
                Kind::Disabled => Action::Disabled,
            };
        }
        ui.add_space(4.0);
        match (kind, &mut new) {
            (Kind::Key, Action::Key { modifiers, key }) => {
                ui.horizontal(|ui| {
                    let label = if self.capturing { "Press a key… (click to cancel)" } else { "🎹 Capture key" };
                    if ui.selectable_label(self.capturing, label).clicked() {
                        self.capturing = !self.capturing;
                    }
                    ui.label("or type a key name:");
                    if self.key_text.is_empty() && *key != 0 && !ui.memory(|m| m.has_focus(egui::Id::unique("g600-keyname"))) {
                        self.key_text = key_name(*key);
                    }
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.key_text)
                            .id(egui::Id::unique("g600-keyname"))
                            .hint_text("e.g. W, Space, F5")
                            .desired_width(110.0),
                    );
                    if r.changed() {
                        let t = self.key_text.trim();
                        if t.is_empty() {
                            *key = 0;
                        } else if let Some(c) = key_code(t) {
                            *key = c;
                        }
                    }
                    if !self.key_text.trim().is_empty() && key_code(self.key_text.trim()).is_none() {
                        ui.colored_label(Color32::from_rgb(0xE0, 0x50, 0x50), "unknown key");
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Modifiers:");
                    for (b, name) in MOD_NAMES.iter().enumerate() {
                        let mut on = *modifiers & (1 << b) != 0;
                        if ui.checkbox(&mut on, *name).changed() {
                            *modifiers ^= 1 << b;
                        }
                    }
                });
                ui.label(
                    RichText::new("Tip: for a modifier on its own (e.g. hold Ctrl to block or duck), tick it and leave the key empty.")
                        .small()
                        .weak(),
                );
                if *modifiers == 0 && *key == 0 {
                    ui.colored_label(Color32::from_rgb(0xE0, 0xA0, 0x30), "Choose a key or a modifier.");
                }
            }
            (Kind::Mouse, Action::Mouse(n)) => {
                ui.horizontal(|ui| {
                    for (k, label) in [(1, "Left"), (2, "Right"), (3, "Middle"), (4, "Back"), (5, "Forward")] {
                        ui.selectable_value(n, k, format!("{k} {label}"));
                    }
                });
            }
            _ => {}
        }
        self.draft = matches!(kind, Kind::Key | Kind::Mouse).then_some(new);
        // An empty combo can't be stored; keep the old value until it's valid.
        let incomplete = new == (Action::Key { modifiers: 0, key: 0 });
        if !incomplete && new != current {
            self.set_button(i, factory, new);
        }
    }

    /// Store an action for button i on the current layer (removing the
    /// override if it equals the factory value).
    fn set_button(&mut self, i: usize, factory: Action, a: Action) {
        let shift = self.shift_layer;
        let entry = self.entry();
        let map = if shift { &mut entry.gshift } else { &mut entry.buttons };
        map.retain(|k, _| button_index(k) != Some(i));
        let short = BUTTON_NAMES[i].split(['/', ' ']).next().unwrap_or(BUTTON_NAMES[i]).to_string();
        if a != factory {
            map.insert(short, a.to_string());
        }
    }

    fn capture_key(&mut self, ctx: &egui::Context) {
        let Some(i) = self.button else { return };
        let events = ctx.input(|inp| inp.events.clone());
        for e in events {
            if let egui::Event::Key { key, physical_key, pressed: true, repeat: false, modifiers, .. } = e {
                if let Some(code) = keymap::hid_code(physical_key.unwrap_or(key)) {
                    let factory = if self.shift_layer { Profile::factory().shift_buttons[i] } else { Profile::factory().buttons[i] };
                    let a = Action::Key { modifiers: keymap::hid_modifiers(modifiers), key: code };
                    self.set_button(i, factory, a);
                    self.draft = Some(a);
                    self.key_text = key_name(code);
                    self.capturing = false;
                    break;
                }
            }
        }
    }

    fn handle_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty() && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
    }

    fn close_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_close {
            return;
        }
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Save your changes before closing?");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        self.save();
                        if !self.dirty() {
                            self.allow_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        self.confirm_close = false;
                    }
                    if ui.button("Discard").clicked() {
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_close = false;
                    }
                });
            });
    }
}

/// Checkbox + colour button for an optional "#RRGGBB" field.
fn color_option(ui: &mut egui::Ui, label: &str, field: &mut Option<String>, effective: Rgb) {
    let mut on = field.is_some();
    if ui.checkbox(&mut on, label).changed() {
        *field = on.then(|| format_rgb(effective));
    }
    if let Some(s) = field {
        let c = parse_rgb(s).unwrap_or(effective);
        let mut rgb = [c.0, c.1, c.2];
        if ui.color_edit_button_srgb(&mut rgb).changed() {
            *s = format_rgb(Rgb(rgb[0], rgb[1], rgb[2]));
        }
    } else {
        ui.label(RichText::new("factory").weak());
    }
}

fn unique_name(existing: &BTreeMap<String, ProfileEntry>, base: &str) -> String {
    if !existing.contains_key(base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base}-{n}")).find(|n| !existing.contains_key(n)).unwrap()
}
