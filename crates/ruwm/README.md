# ruwm

`ruwm` is a minimal Wayland compositor template using Smithay. It creates a
Wayland socket, accepts clients, and exposes the core `wl_compositor` protocol.
It has no output backend, rendering, input, shell protocol, or window management
yet, so it cannot display client windows.

Run it from the workspace root in a Linux session with `XDG_RUNTIME_DIR` set:

```sh
cargo run -p ruwm
```

The program prints its automatically selected socket name (for example,
`wayland-1`). Stop it with Ctrl+C. Clients can select that socket by setting
`WAYLAND_DISPLAY`, but graphical clients need additional compositor protocols
and an output backend before they can show a window.
