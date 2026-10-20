# wset

wset changes keyboard auto-repeat on Wayland, with the same commands as
`xset`:

```sh
wset r rate 210 30
```

waits 210 ms before a held key starts repeating, and then repeats it 30
times a second, just like `xset r rate 210 30` did on X11.

Wayland has no protocol for changing these settings, so wset uses each
desktop's own way.
It supports:

| Desktop | How | Survives a restart |
| --- | --- | --- |
| GNOME | `org.gnome.desktop.peripherals.keyboard` in GSettings | yes |
| Sway | `swaymsg input … repeat_delay/repeat_rate` | no |

`wset q` works on every compositor, because it reads what the compositor
tells applications.

## Usage

```sh
wset [-d] option ...
```

- `r` or `r on` turns auto-repeat on, `r off` or `-r` turns it off.
- `r rate [delay [rate]]` waits `delay` milliseconds (1 to 10000) before a
  held key repeats, then repeats it `rate` times a second (1 to 255), and
  turns auto-repeat on. As with xset, a missing delay means 660 and a
  missing rate 25.
- `q` or `-q` shows the current settings, in the lines `xset q` uses for
  them:

  ```text
  Keyboard Control:
    auto repeat:  on
    auto repeat delay:  210    repeat rate:  30
  ```

- `-d` prints debugging messages to stderr.
- `-h` or `--help` shows a summary, `-V` or `--version` the version.

Options can be combined, as in `wset r rate 210 30 q`.

Like X, GNOME keeps the time between repeats, which xset and wset compute
as 1000 / rate milliseconds; for some rates that makes the rate applications
see a little different, for example 62 for 60.

On GNOME the settings stay after logging out or rebooting, so running wset
once is enough, and it also works in a GNOME X11 session.
Sway forgets them when it restarts; to keep them, put these lines in its
config instead:

```text
input type:keyboard repeat_delay 210
input type:keyboard repeat_rate 30
```

wset sets them for every keyboard, including those that have their own
`input` block in the Sway config.

### Differences from xset

- `-d` turns on debugging messages; it doesn't take a display.
- Auto-repeat for single keys (`r 38`, `-r 38`) is not supported.
- Only keyboard auto-repeat is supported, not the screen saver, DPMS, bell
  and the other xset settings.

## Installation

On Debian 12+ and Ubuntu 22.04+, add the APT repository once:

```sh
sudo apt install -y curl
sudo install -d -m 0755 /etc/apt/keyrings
sudo curl -fsSLo /etc/apt/keyrings/wset.asc https://ksobitov.github.io/wset/wset.asc
sudo chmod 644 /etc/apt/keyrings/wset.asc
sudo tee /etc/apt/sources.list.d/wset.sources <<EOF
Types: deb
URIs: https://ksobitov.github.io/wset
Suites: stable
Components: main
Signed-By: /etc/apt/keyrings/wset.asc
EOF
sudo apt update
sudo apt install wset
```

The repository is signed with the key
`035B 17F4 3B9A A0B0 25D2  4DE5 DA38 B63C 9450 3CEB`, the same as
wbanish's.

Elsewhere, build it with Cargo:

```sh
cargo install --git https://github.com/ksobitov/wset
```

To uninstall:

```sh
sudo apt purge wset
sudo rm /etc/apt/sources.list.d/wset.sources /etc/apt/keyrings/wset.asc
```
