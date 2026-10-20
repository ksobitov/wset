//! Changing keyboard repeat, which Wayland leaves to each compositor.

use std::path::PathBuf;
use std::process::Command;

const GNOME_KEYBOARD: &str = "org.gnome.desktop.peripherals.keyboard";

/// The rate sway uses when nothing else is known.
const SWAY_DEFAULT_RATE: u32 = 25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Desktop {
    Gnome,
    Sway,
}

/// Auto-repeat as the desktop has it configured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub on: bool,
    pub delay: u32,
    /// Repeats per second, also while auto-repeat is off.
    pub rate: u32,
}

impl Desktop {
    pub fn detect() -> Result<Self, String> {
        if std::env::var_os("SWAYSOCK").is_some_and(|s| !s.is_empty()) {
            return Ok(Desktop::Sway);
        }
        let current = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let names: Vec<&str> = current.split(':').collect();
        if names.contains(&"GNOME") {
            Ok(Desktop::Gnome)
        } else if names.iter().any(|n| n.eq_ignore_ascii_case("sway")) {
            Err("SWAYSOCK is not set, so swaymsg can't reach sway; run wset inside the sway session"
                .into())
        } else if current.is_empty() {
            Err("can't tell which desktop this is (XDG_CURRENT_DESKTOP is not set)".into())
        } else {
            Err(format!(
                "changing settings on {current} is not supported yet, only on GNOME and Sway"
            ))
        }
    }

    /// Whether the change survives restarting the compositor.
    pub fn persistent(self) -> bool {
        self == Desktop::Gnome
    }

    pub fn settings(self) -> Result<Settings, String> {
        match self {
            Desktop::Gnome => Ok(Settings {
                on: gsettings_get("repeat")? == "true",
                delay: gsettings_number("delay")?,
                rate: 1000 / gsettings_number("repeat-interval")?.max(1),
            }),
            Desktop::Sway => {
                let k = *sway_keyboards()?.first().ok_or("sway has no keyboard")?;
                let saved = saved_sway_rate().unwrap_or(SWAY_DEFAULT_RATE);
                Ok(Settings {
                    on: k.rate > 0,
                    delay: k.delay,
                    rate: if k.rate > 0 { k.rate } else { saved },
                })
            }
        }
    }

    pub fn set_repeat(self, on: bool) -> Result<(), String> {
        match self {
            Desktop::Gnome => gsettings_set("repeat", if on { "true" } else { "false" }),
            // Sway turns repeat off with a rate of 0, so we keep the old
            // rate ourselves, for as long as the session lasts, like sway.
            Desktop::Sway => {
                let current = self.settings()?;
                if on == current.on {
                    return Ok(());
                }
                if !on && let Some(path) = sway_rate_path() {
                    let _ = std::fs::write(path, current.rate.to_string());
                }
                sway_set(None, Some(if on { current.rate } else { 0 }))
            }
        }
    }

    /// Sets the delay, and the rate if given, and turns auto-repeat on, as
    /// xset does.
    pub fn set_rate(self, delay: u32, rate: Option<u32>) -> Result<(), String> {
        match self {
            Desktop::Gnome => {
                gsettings_set("delay", &delay.to_string())?;
                if let Some(r) = rate {
                    gsettings_set("repeat-interval", &interval(r).to_string())?;
                }
                gsettings_set("repeat", "true")
            }
            // Sway turns repeat on with any rate above 0.
            Desktop::Sway => {
                let rate = match rate {
                    Some(r) => r,
                    None => self.settings()?.rate,
                };
                sway_set(Some(delay), Some(rate))
            }
        }
    }

    /// The rate applications will be told after asking for `rate`.
    pub fn effective_rate(self, rate: u32) -> u32 {
        match self {
            Desktop::Gnome => 1000 / interval(rate),
            Desktop::Sway => rate,
        }
    }
}

/// GNOME, like X, keeps the milliseconds between repeats, and xset turns a
/// rate into them the same way: 1000 / rate.
fn interval(rate: u32) -> u32 {
    (1000 / rate.max(1)).max(1)
}

fn gsettings_get(key: &str) -> Result<String, String> {
    let out = run("gsettings", &["get", GNOME_KEYBOARD, key])?;
    Ok(out.trim().to_owned())
}

/// Reads a uint32 key, which gsettings prints as "uint32 500".
fn gsettings_number(key: &str) -> Result<u32, String> {
    let value = gsettings_get(key)?;
    value
        .split_whitespace()
        .last()
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| format!("unexpected value for {key}: {value}"))
}

/// Sets a key and reads it back, because gsettings without a session bus
/// "succeeds" while keeping the value in memory only.
fn gsettings_set(key: &str, value: &str) -> Result<(), String> {
    run("gsettings", &["set", GNOME_KEYBOARD, key, value])?;
    let stored = gsettings_get(key)?;
    if stored.split_whitespace().last() == Some(value) {
        Ok(())
    } else {
        Err(format!("GSettings didn't keep {key} = {value} (no session D-Bus?)"))
    }
}

#[derive(Clone, Copy, Debug)]
struct SwayKeyboard {
    delay: u32,
    rate: u32,
}

/// Sway's keyboards, with their identifiers.
fn sway_inputs() -> Result<Vec<(String, SwayKeyboard)>, String> {
    let out = run("swaymsg", &["-t", "get_inputs", "-r"])?;
    let inputs: serde_json::Value =
        serde_json::from_str(&out).map_err(|e| format!("swaymsg -t get_inputs: {e}"))?;
    let number = |v: &serde_json::Value, key: &str| v[key].as_u64().unwrap_or(0) as u32;
    Ok(inputs
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| i["type"] == "keyboard")
        .map(|i| {
            let id = i["identifier"].as_str().unwrap_or_default().to_owned();
            (id, SwayKeyboard { delay: number(i, "repeat_delay"), rate: number(i, "repeat_rate") })
        })
        .collect())
}

fn sway_keyboards() -> Result<Vec<SwayKeyboard>, String> {
    Ok(sway_inputs()?.into_iter().map(|(_, k)| k).collect())
}

/// Sets the type:keyboard defaults, which keyboards plugged in later get,
/// and every current keyboard, since one with its own input block in the
/// config would ignore the defaults.  Then checks that sway took them.
fn sway_set(delay: Option<u32>, rate: Option<u32>) -> Result<(), String> {
    let keyboards = sway_inputs()?;
    let mut targets = vec!["type:keyboard".to_owned()];
    targets.extend(keyboards.iter().map(|(id, _)| format!("\"{id}\"")));
    let mut commands = Vec::new();
    for target in &targets {
        if let Some(d) = delay {
            commands.push(format!("input {target} repeat_delay {d}"));
        }
        if let Some(r) = rate {
            commands.push(format!("input {target} repeat_rate {r}"));
        }
    }
    run("swaymsg", &[&commands.join("; ")])?;
    let wrong = sway_keyboards()?
        .into_iter()
        .any(|k| delay.is_some_and(|d| k.delay != d) || rate.is_some_and(|r| k.rate != r));
    if wrong {
        return Err("sway didn't apply the new repeat settings to every keyboard".into());
    }
    Ok(())
}

fn sway_rate_path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|d| !d.is_empty())
        .map(|d| PathBuf::from(d).join("wset-sway-rate"))
}

fn saved_sway_rate() -> Option<u32> {
    std::fs::read_to_string(sway_rate_path()?).ok()?.trim().parse().ok().filter(|&r| r > 0)
}

/// Runs a command and returns its output, or what went wrong.
fn run(program: &str, args: &[&str]) -> Result<String, String> {
    debug!("running {program} {}", args.join(" "));
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("can't run {program}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let msg = String::from_utf8_lossy(&out.stdout);
        Err(format!(
            "{program} {}: {}",
            args.join(" "),
            if err.trim().is_empty() { msg.trim() } else { err.trim() }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_like_xset() {
        // xset sends 1000 / rate, and GNOME reports 1000 / interval.
        assert_eq!(interval(30), 33);
        assert_eq!(Desktop::Gnome.effective_rate(30), 30);
        assert_eq!(Desktop::Gnome.effective_rate(25), 25);
        assert_eq!(Desktop::Gnome.effective_rate(7), 7);
        assert_eq!(Desktop::Gnome.effective_rate(35), 35);
        assert_eq!(Desktop::Gnome.effective_rate(15), 15);
        assert_eq!(Desktop::Gnome.effective_rate(60), 62);
        assert_eq!(interval(255), 3);
        assert_eq!(interval(1), 1000);
    }
}
