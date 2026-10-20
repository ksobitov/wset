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

use desktop::{Desktop, Settings};
use query::Repeat;

const USAGE: &str = "\
usage: wset [-d] option ...
    r [on|off]               turn keyboard auto-repeat on or off
    -r                       turn keyboard auto-repeat off
    r rate [delay [rate]]    wait delay milliseconds (660 if not given) before a
                             held key repeats, then repeat it rate times a
                             second (25 if not given), and turn auto-repeat on
    q, -q                    show the current settings
    -d                       print debugging messages
    -h, --help               show this help
    -V, --version            show the version";

/// xset's defaults for "r rate".
const DEFAULT_DELAY: u32 = 660;
const DEFAULT_RATE: u32 = 25;

#[derive(Debug, PartialEq)]
enum Action {
    Repeat(bool),
    Rate(u32, u32),
    Query,
}

#[derive(Debug, PartialEq)]
enum Parsed {
    Run(bool, Vec<Action>),
    Help,
    Version,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Parsed, String> {
    let mut debug = false;
    let mut actions = Vec::new();
    let mut args = args.into_iter().peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-d" => debug = true,
            "-h" | "--help" => return Ok(Parsed::Help),
            "-V" | "--version" => return Ok(Parsed::Version),
            "q" | "-q" => actions.push(Action::Query),
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
                    // Like xset: a rate only after a delay, defaults for the rest.
                    let delay = number(&mut args, "delay", 1..=10_000)?;
                    let rate = match delay {
                        Some(_) => number(&mut args, "rate", 1..=255)?,
                        None => None,
                    };
                    actions.push(Action::Rate(
                        delay.unwrap_or(DEFAULT_DELAY),
                        rate.unwrap_or(DEFAULT_RATE),
                    ));
                }
                Some(n) if n.parse::<u32>().is_ok() => {
                    return Err("auto-repeat for single keys is not supported".into());
                }
                _ => actions.push(Action::Repeat(true)),
            },
            "b" | "-b" | "bc" | "-bc" | "c" | "-c" | "dpms" | "-dpms" | "+dpms" | "fp" | "-fp"
            | "+fp" | "fp-" | "fp+" | "fp=" | "fp+=" | "led" | "-led" | "m" | "p" | "s" | "-k"
            | "+k" => {
                return Err(format!("'{arg}' from xset is not supported"));
            }
            _ => return Err(format!("unknown option '{arg}'")),
        }
    }
    if actions.is_empty() {
        return Ok(Parsed::Help);
    }
    Ok(Parsed::Run(debug, actions))
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
    let args: Result<Vec<String>, _> =
        std::env::args_os().skip(1).map(|a| a.into_string()).collect();
    let parsed = match args {
        Ok(args) => parse_args(args),
        Err(arg) => Err(format!("unexpected argument {arg:?}")),
    };
    let (debug, actions) = match parsed {
        Ok(Parsed::Run(d, a)) => (d, a),
        Ok(Parsed::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(Parsed::Version) => {
            println!("wset {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("wset: {e}");
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
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

/// What a later "q" should see once the compositor has picked up a change.
type Expected = Box<dyn Fn(Repeat) -> bool>;

fn run(actions: &[Action]) -> Result<(), String> {
    let mut desktop = None;
    let mut expected: Option<Expected> = None;
    for action in actions {
        if *action != Action::Query && desktop.is_none() {
            let d = Desktop::detect()?;
            debug!("desktop: {d:?}");
            desktop = Some(d);
        }
        match *action {
            Action::Repeat(on) => {
                desktop.unwrap().set_repeat(on)?;
                expected = Some(Box::new(move |r| (r.rate > 0) == on));
            }
            Action::Rate(delay, rate) => {
                let d = desktop.unwrap();
                d.set_rate(delay, rate)?;
                let rate = d.effective_rate(rate) as i32;
                expected = Some(Box::new(move |r| r.delay == delay as i32 && r.rate == rate));
            }
            Action::Query => {
                let desktop = desktop.or_else(|| Desktop::detect().ok());
                print(show(desktop, expected.take())?);
            }
        }
    }

    if let Some(d) = desktop
        && !d.persistent()
        && std::io::stderr().is_terminal()
    {
        eprintln!(
            "wset: sway forgets this when it restarts; for good, put \
             'input type:keyboard repeat_delay ...' and 'repeat_rate ...' in its config"
        );
    }
    Ok(())
}

/// What "q" prints: the compositor's view where there is one, waiting up to
/// a second for it to show `expected`, else the desktop's settings.
fn show(desktop: Option<Desktop>, expected: Option<Expected>) -> Result<Option<Settings>, String> {
    let deadline = Instant::now() + Duration::from_secs(1);
    let repeat = loop {
        let repeat = match query::repeat() {
            Ok(r) => r,
            // No Wayland, say in a GNOME X11 session: GSettings still knows.
            Err(e) => match desktop {
                Some(d) => return d.settings().map(Some),
                None => return Err(e),
            },
        };
        let done = match (&expected, repeat) {
            (Some(check), Some(r)) => check(r),
            _ => true,
        };
        if done || Instant::now() >= deadline {
            break repeat;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    Ok(match repeat {
        Some(r) if r.rate > 0 => {
            Some(Settings { on: true, delay: r.delay as u32, rate: r.rate as u32 })
        }
        // While repeat is off the compositor may not report its numbers.
        Some(r) => Some(desktop.and_then(|d| d.settings().ok()).unwrap_or(Settings {
            on: false,
            delay: r.delay as u32,
            rate: 0,
        })),
        None => desktop.and_then(|d| d.settings().ok()),
    })
}

/// Prints the settings in the lines xset q uses for them.
fn print(settings: Option<Settings>) {
    println!("Keyboard Control:");
    match settings {
        Some(s) => {
            println!("  auto repeat:  {}", if s.on { "on" } else { "off" });
            println!("  auto repeat delay:  {}    repeat rate:  {}", s.delay, s.rate);
        }
        None => println!("  no keyboard"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Parsed, String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    fn run_(debug: bool, actions: Vec<Action>) -> Result<Parsed, String> {
        Ok(Parsed::Run(debug, actions))
    }

    #[test]
    fn args() {
        use Action::*;
        assert_eq!(parse(&["r", "rate", "210", "30"]), run_(false, vec![Rate(210, 30)]));
        assert_eq!(parse(&["r", "rate", "210", "q"]), run_(false, vec![Rate(210, 25), Query]));
        assert_eq!(parse(&["r", "rate"]), run_(false, vec![Rate(660, 25)]));
        assert_eq!(parse(&["-d", "r", "-q"]), run_(true, vec![Repeat(true), Query]));
        assert_eq!(
            parse(&["r", "off", "-r", "r", "on"]),
            run_(false, vec![Repeat(false), Repeat(false), Repeat(true)])
        );
        assert_eq!(parse(&[]), Ok(Parsed::Help));
        assert_eq!(parse(&["--help"]), Ok(Parsed::Help));
        assert_eq!(parse(&["--version"]), Ok(Parsed::Version));
        assert!(parse(&["r", "rate", "0"]).is_err());
        assert!(parse(&["r", "rate", "210", "256"]).is_err());
        assert!(parse(&["r", "rate", "10001"]).is_err());
        assert!(parse(&["r", "38"]).is_err());
        assert!(parse(&["s", "off"]).is_err());
        assert!(parse(&["x"]).is_err());
    }
}
