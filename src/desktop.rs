//! Changing keyboard repeat, which Wayland leaves to each compositor.

use std::process::Command;

use crate::query::Repeat;

const GNOME_KEYBOARD: &str = "org.gnome.desktop.peripherals.keyboard";

/// Sway's defaults, which it has no command to go back to.
const SWAY_DEFAULT: Repeat = Repeat { delay: 600, rate: 25 };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Desktop {
    Gnome,
    Sway,
}

impl Desktop {
    pub fn detect() -> Result<Self, String> {
        let current = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        if current.split(':').any(|d| d == "GNOME") {
            Ok(Desktop::Gnome)
        } else if std::env::var_os("SWAYSOCK").is_some() {
            Ok(Desktop::Sway)
        } else if current.is_empty() {
            Err("can't tell which desktop this is (XDG_CURRENT_DESKTOP is not set)".into())
        } else {
            Err(format!("changing settings on {current} is not supported yet"))
        }
    }

    /// Whether the change survives restarting the compositor.
    pub fn persistent(self) -> bool {
        self == Desktop::Gnome
    }

    pub fn set_repeat(self, on: bool, current: Option<Repeat>) -> Result<(), String> {
        match self {
            Desktop::Gnome => gsettings(&["set", GNOME_KEYBOARD, "repeat", &on.to_string()]),
            // Sway turns repeat off with a rate of 0, so we keep the old
            // rate ourselves, for as long as the session lasts, like sway.
            Desktop::Sway if on => match current {
                Some(r) if r.rate > 0 => Ok(()),
                _ => swaymsg("repeat_rate", saved_sway_rate().unwrap_or(SWAY_DEFAULT.rate)),
            },
            Desktop::Sway => {
                if let Some(r) = current.filter(|r| r.rate > 0)
                    && let Some(path) = sway_rate_path()
                {
                    let _ = std::fs::write(path, r.rate.to_string());
                }
                swaymsg("repeat_rate", 0)
            }
        }
    }

    /// Sets the delay and the rate, or the desktop's defaults for both
    /// when neither is given.
    pub fn set_rate(self, delay: Option<u32>, rate: Option<u32>) -> Result<(), String> {
        let defaults = delay.is_none() && rate.is_none();
        match self {
            Desktop::Gnome if defaults => {
                gsettings(&["reset", GNOME_KEYBOARD, "delay"])?;
                gsettings(&["reset", GNOME_KEYBOARD, "repeat-interval"])
            }
            Desktop::Gnome => {
                if let Some(d) = delay {
                    gsettings(&["set", GNOME_KEYBOARD, "delay", &d.to_string()])?;
                }
                if let Some(r) = rate {
                    let interval = gnome_interval(r).to_string();
                    gsettings(&["set", GNOME_KEYBOARD, "repeat-interval", &interval])?;
                }
                Ok(())
            }
            Desktop::Sway => {
                let (d, r) = if defaults {
                    (Some(SWAY_DEFAULT.delay as u32), Some(SWAY_DEFAULT.rate as u32))
                } else {
                    (delay, rate)
                };
                if let Some(d) = d {
                    swaymsg("repeat_delay", d as i32)?;
                }
                if let Some(r) = r {
                    swaymsg("repeat_rate", r as i32)?;
                }
                Ok(())
            }
        }
    }

    /// The rate clients will be told after asking for `rate`.
    pub fn effective_rate(self, rate: u32) -> i32 {
        match self {
            Desktop::Gnome => (1000 / gnome_interval(rate)) as i32,
            Desktop::Sway => rate as i32,
        }
    }
}

/// GNOME stores the milliseconds between repeats, and hands clients
/// 1000 / interval as the rate.
fn gnome_interval(rate: u32) -> u32 {
    ((1000.0 / rate as f64).round() as u32).max(1)
}

fn sway_rate_path() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|d| std::path::Path::new(&d).join("wset-sway-rate"))
}

fn saved_sway_rate() -> Option<i32> {
    std::fs::read_to_string(sway_rate_path()?).ok()?.trim().parse().ok().filter(|&r| r > 0)
}

fn gsettings(args: &[&str]) -> Result<(), String> {
    run("gsettings", args)
}

fn swaymsg(setting: &str, value: i32) -> Result<(), String> {
    run("swaymsg", &["input", "type:keyboard", setting, &value.to_string()])
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    debug!("running {program} {}", args.join(" "));
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("can't run {program}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("{program} {}: {}", args.join(" "), err.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnome_rates() {
        assert_eq!(gnome_interval(30), 33);
        assert_eq!(Desktop::Gnome.effective_rate(30), 30);
        assert_eq!(Desktop::Gnome.effective_rate(25), 25);
        assert_eq!(gnome_interval(5000), 1);
    }
}
