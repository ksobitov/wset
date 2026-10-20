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
| Sway | `swaymsg input type:keyboard …` | no |

`wset q` works on every compositor, because it reads what the compositor
tells applications.

## Usage

```sh
wset [-d] option ...
```

- `r` or `r on` turns auto-repeat on, `r off` or `-r` turns it off.
- `r rate [delay [rate]]` sets the delay in milliseconds and the rate in
  repeats per second; without values it goes back to the desktop's
  defaults, and with only a delay the rate stays as it is.
- `q` shows the current settings, the way `xset q` does:

  ```text
  Keyboard Control:
    auto repeat:  on    auto repeat delay:  210    repeat rate:  30
  ```

- `-d` prints debugging messages to stderr.

Options can be combined, as in `wset r rate 210 30 q`.

On GNOME the settings stay after logging out or rebooting, so running wset
once is enough.
Sway forgets them when it restarts; put
`input type:keyboard { repeat_delay 210; repeat_rate 30 }` in its config, or
`exec wset r rate 210 30`.

## Installation

On Debian 12+ and Ubuntu 22.04+, add the APT repository once:

```sh
sudo apt install -y curl
sudo install -d -m 0755 /etc/apt/keyrings
sudo curl -fsSLo /etc/apt/keyrings/wset.asc https://ksobitov.github.io/wset/wset.asc
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

Elsewhere, build it with Cargo:

```sh
cargo install --git https://github.com/ksobitov/wset
```
