# rumenu

A small desktop application launcher for X11 and Wayland compositors supporting `wlr-layer-shell`.

Run `rumenu` once in your session. It remains hidden until `rumenu toggle` is called. Bind `rumenu toggle` to a compositor or window manager shortcut. Escape closes the launcher, Enter launches the selected app, and the arrow keys select a result. Arrow-key selection wraps between the first and last results. Hold a key to repeat text entry, deletion, or selection movement at the keyboard's configured rate. Click a result to launch it.

rumenu reads desktop entries from the XDG applications directories. Search matches application names, descriptions, and keywords. Terminal applications appear only when a terminal command is configured.

Configuration is read from `$XDG_CONFIG_HOME/rumenu/config.toml` (normally `~/.config/rumenu/config.toml`) whenever the launcher is shown. For example:

```toml
width = 560
height = 420
max_results = 8
font_family = "sans-serif"
font_size = 16.0
icon_theme = "hicolor"
terminal = ["foot", "-e"]
background = "#24283b"
foreground = "#c0caf5"
selection = "#414868"
muted = "#9aa5ce"
```

The daemon and `toggle` command communicate through a per-display socket in `$XDG_RUNTIME_DIR`.

## Supported platforms

`rumenu` currently supports Linux and FreeBSD. The long-term goal is to support platforms that run X11 or Wayland, including Linux, the BSD family, illumos, and GNU Hurd.
