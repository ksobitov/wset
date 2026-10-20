//! wset: change keyboard repeat on Wayland, like xset does on X11.

use std::io::IsTerminal;
use std::iter::Peekable;
use std::ops::RangeInclusive;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static DEBUG: AtomicBool = AtomicBool::new(false);

macro_rules! debug {
    ($($arg:tt)*) => {
        if crate::DEBUG.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!("wset: {}", format_args!($($arg)*));
        }
    };
}

mod desktop;
mod query;

use desktop::Desktop;
use query::Repeat;

const USAGE: &str = "\
usage: wset [-d] option ...
    r [on|off]               turn keyboard auto-repeat on or off
    -r                       turn keyboard auto-repeat off
    r rate [delay [rate]]    set the auto-repeat delay in milliseconds and the
                             rate in repeats per second; without values, go
                             back to the desktop's defaults
    q                        show the current settings";

#[derive(Debug, PartialEq)]
enum Action {
    Repeat(bool),
    Rate(Option<u32>, Option<u32>),
    Query,
}

/// Returns whether -d was given, and the actions in order.
fn parse_args(args: impl IntoIterator<Item = String>) -> Result<(bool, Vec<Action>), String> {
    let mut debug = false;
    let mut actions = Vec::new();
    let mut args = args.into_iter().peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-d" => debug = true,
            "-h" | "--help" => return Err(String::new()),
            "-V" | "--version" => {
                println!("wset {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "q" => actions.push(Action::Query),
            "-r" => actions.push(Action::Repeat(false)),
            "r" => match args.peek().map(String::as_str) {
                Some("on") => {
                    args.next();
                    actions.push(Action::Repeat(true));
                }
                Some("off") => {
                    args.next();
                    actions.push(Action::Repeat(false));
                }
                Some("rate") => {
                    args.next();
                    let delay = number(&mut args, "delay", 1..=10_000)?;
                    let rate = match delay {
                        Some(_) => number(&mut args, "rate", 1..=1000)?,
                        None => None,
                    };
                    actions.push(Action::Rate(delay, rate));
                }
                Some(n) if n.parse::<u32>().is_ok() => {
                    return Err("auto-repeat for single keys is not supported".into());
                }
                _ => actions.push(Action::Repeat(true)),
            },
            "b" | "-b" | "bc" | "-bc" | "c" | "-c" | "dpms" | "-dpms" | "+dpms" | "fp" | "-fp"
            | "+fp" | "fp-" | "fp+" | "fp=" | "fp+=" | "led" | "-led" | "m" | "p" | "s" => {
                return Err(format!("'{arg}' from xset is not supported yet"));
            }
            _ => return Err(format!("unknown option '{arg}'")),
        }
    }
    if actions.is_empty() {
        return Err(String::new());
    }
    Ok((debug, actions))
}

/// Takes the next argument if it is a number, as xset does.
fn number<I: Iterator<Item = String>>(
    args: &mut Peekable<I>,
    what: &str,
    range: RangeInclusive<u32>,
) -> Result<Option<u32>, String> {
    let Some(n) = args.peek().and_then(|a| a.parse::<u32>().ok()) else {
        return Ok(None);
    };
    args.next();
    if range.contains(&n) {
        Ok(Some(n))
    } else {
        Err(format!("{what} must be from {} to {}", range.start(), range.end()))
    }
}

fn main() -> ExitCode {
    let (debug, actions) = match parse_args(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("wset: {e}");
            }
            eprintln!("{USAGE}");
            return ExitCode::from(if e.is_empty() { 0 } else { 1 });
        }
    };
    DEBUG.store(debug, Ordering::Relaxed);
    match run(&actions) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("wset: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(actions: &[Action]) -> Result<(), String> {
    let mut desktop = None;
    // What the last change should show, so a later "q" can wait for the
    // compositor to pick it up.
    let mut changed: Option<Box<dyn Fn(Repeat) -> bool>> = None;
    let mut before = None;

    for action in actions {
        if *action != Action::Query && desktop.is_none() {
            let d = Desktop::detect()?;
            debug!("desktop: {d:?}");
            desktop = Some(d);
            if actions.contains(&Action::Query) {
                before = query::repeat()?;
            }
        }
        match *action {
            Action::Repeat(on) => {
                let current = query::repeat()?;
                desktop.unwrap().set_repeat(on, current)?;
                changed = Some(Box::new(move |r| (r.rate > 0) == on));
            }
            Action::Rate(delay, rate) => {
                let d = desktop.unwrap();
                d.set_rate(delay, rate)?;
                changed = Some(match (delay, rate) {
                    (None, None) => Box::new(move |r| Some(r) != before),
                    _ => Box::new(move |r| {
                        delay.is_none_or(|d| r.delay == d as i32)
                            && rate.is_none_or(|x| r.rate == d.effective_rate(x))
                    }),
                });
            }
            Action::Query => print(wait_for(changed.take())?),
        }
    }

    if let Some(d) = desktop
        && !d.persistent()
        && std::io::stderr().is_terminal()
    {
        eprintln!("wset: {d:?} forgets this when it restarts; run wset from its config");
    }
    Ok(())
}

/// Queries the compositor, giving it up to a second to show `changed`.
fn wait_for(changed: Option<Box<dyn Fn(Repeat) -> bool>>) -> Result<Option<Repeat>, String> {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let repeat = query::repeat()?;
        let done = match (&changed, repeat) {
            (Some(check), Some(r)) => check(r),
            _ => true,
        };
        if done || Instant::now() >= deadline {
            return Ok(repeat);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn print(repeat: Option<Repeat>) {
    println!("Keyboard Control:");
    match repeat {
        Some(r) => println!(
            "  auto repeat:  {}    auto repeat delay:  {}    repeat rate:  {}",
            if r.rate > 0 { "on" } else { "off" },
            r.delay,
            r.rate
        ),
        None => println!("  no keyboard"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<(bool, Vec<Action>), String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn args() {
        use Action::*;
        assert_eq!(
            parse(&["r", "rate", "210", "30"]),
            Ok((false, vec![Rate(Some(210), Some(30))]))
        );
        assert_eq!(
            parse(&["r", "rate", "210", "q"]),
            Ok((false, vec![Rate(Some(210), None), Query]))
        );
        assert_eq!(parse(&["r", "rate"]), Ok((false, vec![Rate(None, None)])));
        assert_eq!(parse(&["-d", "r", "q"]), Ok((true, vec![Repeat(true), Query])));
        assert_eq!(
            parse(&["r", "off", "-r", "r", "on"]),
            Ok((false, vec![Repeat(false), Repeat(false), Repeat(true)]))
        );
        assert!(parse(&[]).is_err());
        assert!(parse(&["r", "rate", "0"]).is_err());
        assert!(parse(&["r", "rate", "210", "5000"]).is_err());
        assert!(parse(&["r", "38"]).is_err());
        assert!(parse(&["s", "off"]).is_err());
        assert!(parse(&["x"]).is_err());
    }
}
