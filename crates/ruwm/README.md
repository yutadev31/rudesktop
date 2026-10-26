# ruwm

`ruwm` is a minimal Wayland compositor template using Smithay. It creates a
Wayland socket, accepts clients, and exposes `wl_compositor` and `wl_output`.
Its winit backend opens a window in an existing Wayland or X11 session and
renders an empty background. It does not yet implement input, a shell protocol,
or window management, so it cannot display client windows.

Run it from the workspace root in a Linux graphical session with
`XDG_RUNTIME_DIR` and either `WAYLAND_DISPLAY` or `DISPLAY` set:

```sh
cargo run -p ruwm
```

The program prints its automatically selected socket name (for example,
`wayland-1`). Close the winit window or press Ctrl+C to stop it. Clients can
select the printed socket by setting `WAYLAND_DISPLAY`. Graphical clients need
additional compositor protocols before they can show a window.
