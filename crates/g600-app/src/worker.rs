//! Background thread that owns the mouse. Receives foreground changes,
//! debounces them, and puts the right profile on the mouse.
//!
//! Safety: all writes go through `G600::write_profile` (busy-wait, never the
//! active slot). After any write problem the worker halts and makes no more
//! changes until restarted: the user should replug and check with `g600 dump`.

use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::PathBuf,
    sync::mpsc::{Receiver, RecvTimeoutError, Sender},
    time::{Duration, SystemTime},
};

use g600_core::{device::G600, profile::Profile, PROFILE_REPORT_IDS};

use crate::foreground;
use hidapi::HidApi;

use g600_app::{
    config::{Config, NamedProfile, DEFAULT_DEBOUNCE_MS},

    slots::{Content, Plan, SlotState},
};

/// How often to double-check the foreground app when no events arrive.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

pub enum Msg {
    Foreground(String),
    Reload,
    SetPaused(bool),
    Quit,
}

pub struct Paths {
    pub config: PathBuf,
    pub log: PathBuf,
    pub backups: PathBuf,
}

struct Device {
    g: G600,
    slots: SlotState,
}

pub struct Worker {
    paths: Paths,
    status: Sender<String>,
    config: Option<Config>,
    config_mtime: Option<SystemTime>,
    device: Option<Device>,
    api: Option<HidApi>,
    halted: Option<String>,
    paused: bool,
    last_exe: Option<String>,
}

impl Worker {
    pub fn new(paths: Paths, status: Sender<String>) -> Self {
        Worker {
            paths,
            status,
            config: None,
            config_mtime: None,
            device: None,
            api: None,
            halted: None,
            paused: false,
            last_exe: None,
        }
    }

    pub fn run(mut self, rx: Receiver<Msg>) {
        self.reload(true);
        loop {
            let msg = match rx.recv_timeout(POLL_INTERVAL) {
                Ok(m) => m,
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {
                    // Foreground events can go missing (e.g. after Alt-Tab into
                    // some apps), so also check the real foreground regularly.
                    if let Some(exe) = foreground::current() {
                        if self.last_exe.as_deref() != Some(exe.as_str()) {
                            self.log(&format!("poll: foreground is {exe} (no event seen)"));
                            self.apply(exe);
                        }
                    }
                    continue;
                }
            };
            let mut pending = match msg {
                Msg::Foreground(exe) => Some(exe),
                other => match self.control(other) {
                    Flow::Quit => return,
                    Flow::Reapply => self.last_exe.clone(),
                    Flow::Continue => None,
                },
            };
            let Some(first) = pending.take() else { continue };
            // Debounce: alt-tab and popups fire several events in a row.
            let mut exe = first;
            let debounce = Duration::from_millis(
                self.config.as_ref().map_or(DEFAULT_DEBOUNCE_MS, |c| c.debounce_ms),
            );
            loop {
                match rx.recv_timeout(debounce) {
                    Ok(Msg::Foreground(e)) => exe = e,
                    Ok(other) => {
                        if let Flow::Quit = self.control(other) {
                            return;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            // Trust what is really in front now over the last event seen.
            let exe = foreground::current().unwrap_or(exe);
            self.apply(exe);
        }
    }

    fn control(&mut self, msg: Msg) -> Flow {
        match msg {
            Msg::Quit => Flow::Quit,
            Msg::Reload => {
                self.reload(true);
                Flow::Reapply
            }
            Msg::SetPaused(p) => {
                self.paused = p;
                self.log(if p { "paused" } else { "resumed" });
                if p {
                    self.set_status("Paused".into());
                    Flow::Continue
                } else {
                    Flow::Reapply
                }
            }
            Msg::Foreground(_) => Flow::Continue,
        }
    }

    fn apply(&mut self, exe: String) {
        self.last_exe = Some(exe.clone());
        if self.paused {
            return;
        }
        if let Some(why) = &self.halted {
            self.set_status(format!("STOPPED: {why}"));
            return;
        }
        self.reload(false);
        let Some(cfg) = &self.config else { return };
        let np = cfg.profile_for(&exe).clone();
        match self.switch_to(&np) {
            Ok(Some(slot)) => self.set_status(format!("{} (slot {}) for {exe}", np.name, slot + 1)),
            Ok(None) => {}
            Err(Failure::Device(e)) => {
                self.log(&format!("device problem: {e}"));
                self.device = None; // reopen next time
                self.set_status(format!("Mouse not available: {e}"));
            }
            Err(Failure::Halt(e)) => {
                self.log(&format!("HALT: {e}"));
                self.set_status(format!("STOPPED: {e}"));
                self.halted = Some(e);
                self.device = None;
            }
        }
    }

    /// Returns the slot now active, or None if the mouse was not reachable.
    fn switch_to(&mut self, np: &NamedProfile) -> Result<Option<usize>, Failure> {
        let content = np.profile.content().map_err(|e| Failure::Halt(e.to_string()))?;
        self.ensure_device()?;
        let dev = self.device.as_mut().expect("ensure_device succeeded");
        let active = dev.g.status().map_err(Failure::Device)?.profile as usize;
        if active > 2 {
            return Err(Failure::Device(format!("mouse reports active profile {active}")));
        }
        let slot = match dev.slots.plan(&content, active) {
            Plan::Nothing => active,
            Plan::Select(s) => {
                activate(&dev.g, s, &np.profile).map_err(Failure::Device)?;
                s
            }
            Plan::WriteThenSelect(s) => {
                let mut p = np.profile.clone();
                p.report_id = PROFILE_REPORT_IDS[s];
                let bytes = p.encode().map_err(|e| Failure::Halt(e.to_string()))?;
                let msg = format!("writing profile '{}' to slot {}", np.name, s + 1);
                self.log(&msg);
                let dev = self.device.as_mut().unwrap();
                match dev.g.write_profile(&bytes) {
                    Ok(true) => {}
                    Ok(false) => {
                        return Err(Failure::Halt(format!(
                            "read-back after writing slot {} differed; replug and check with `g600 dump`",
                            s + 1
                        )))
                    }
                    Err(e) => return Err(Failure::Halt(format!("writing slot {}: {e}", s + 1))),
                }
                dev.slots.content[s] = content;
                activate(&dev.g, s, &np.profile).map_err(Failure::Device)?;
                s
            }
        };
        let dev = self.device.as_mut().unwrap();
        dev.slots.mark_used(slot);
        Ok(Some(slot))
    }

    /// Open the mouse if needed, read what the 3 slots hold, back them up.
    fn ensure_device(&mut self) -> Result<(), Failure> {
        if self.device.is_some() {
            return Ok(());
        }
        let api = match self.api.take() {
            Some(mut a) => {
                a.refresh_devices().map_err(|e| Failure::Device(e.to_string()))?;
                a
            }
            None => HidApi::new().map_err(|e| Failure::Device(e.to_string()))?,
        };
        let opened = G600::open(&api);
        self.api = Some(api);
        let g = opened.map_err(Failure::Device)?;
        let mut content: [Content; 3] = [[0; 153]; 3];
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let dir = self.paths.backups.join(stamp.to_string());
        let _ = fs::create_dir_all(&dir);
        for (s, &id) in PROFILE_REPORT_IDS.iter().enumerate() {
            let raw = g.read_report(id).map_err(|e| Failure::Device(e.to_string()))?;
            let _ = fs::write(dir.join(format!("{id:02X}.bin")), &raw);
            // Undecodable slot contents just never match any profile.
            if let Ok(p) = Profile::decode(&raw) {
                if let Ok(c) = p.content() {
                    content[s] = c;
                }
            }
        }
        self.log(&format!("opened mouse; slot backup in {}", dir.display()));
        self.device = Some(Device { g, slots: SlotState::new(content) });
        Ok(())
    }

    fn reload(&mut self, force: bool) {
        let mtime = fs::metadata(&self.paths.config).and_then(|m| m.modified()).ok();
        if !force && mtime == self.config_mtime && self.config.is_some() {
            return;
        }
        self.config_mtime = mtime;
        match Config::load(&self.paths.config) {
            Ok(c) => {
                let names: Vec<&str> = c.profiles.iter().map(|p| p.name.as_str()).collect();
                self.log(&format!("config loaded: {}", names.join(", ")));
                self.config = Some(c);
            }
            Err(e) => {
                self.log(&format!("config error:\n{e}"));
                // Keep using the previous good config, if any.
                let keep = if self.config.is_some() { " (still using the previous config)" } else { "" };
                self.set_status(format!("Config error{keep}: {}", e.lines().next().unwrap_or("")));
            }
        }
    }

    fn set_status(&self, s: String) {
        self.log(&s);
        let _ = self.status.send(s);
    }

    fn log(&self, line: &str) {
        let t = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&self.paths.log) {
            let _ = writeln!(f, "[{t}] {line}");
        }
        if cfg!(debug_assertions) {
            eprintln!("{line}");
        }
    }
}

enum Flow {
    Continue,
    Reapply,
    Quit,
}

enum Failure {
    /// Mouse unplugged or unreachable: drop the handle, retry later.
    Device(String),
    /// A write went wrong: stop changing the mouse entirely.
    Halt(String),
}

fn activate(g: &G600, slot: usize, p: &Profile) -> Result<(), String> {
    g.select_profile(slot as u8)?;
    if (1..=4).contains(&p.dpi_default) {
        g.apply_resolution(p.dpi_default - 1)?;
    }
    Ok(())
}
