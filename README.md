# rudesktop

`rudesktop` is a Rust workspace for desktop tools and shared libraries. Its applications cover a status bar, application launcher, screenshot capture, and clipboard access.

| Package | Purpose |
| --- | --- |
| [`rubar`](crates/rubar/README.md) | Configurable status bar for Wayland and X11. |
| [`rumenu`](crates/rumenu/README.md) | Application launcher for X11 and Wayland compositors with `wlr-layer-shell`. |
| [`shot`](crates/shot/README.md) | Monitor screenshots saved as PNG and copied to the clipboard. |
| [`cpst`](crates/cpst/README.md) | Clipboard commands: `cpy` copies input and `pst` pastes clipboard contents. |
| [`shell-surface`](crates/shell-surface/) | Shared Wayland and X11 surface and input abstraction. |
| [`desktop-icons`](crates/desktop-icons/) | Shared desktop icon lookup and PNG/SVG decoding. |

## Build and run

The Nix development shell provides Rust and the native Linux build dependencies:

```sh
nix develop
cargo build --workspace --locked
```

Run an application from the repository root:

```sh
cargo run -p rubar
cargo run -p rumenu
cargo run -p shot -- --help
```

The Nix flake provides packages for `cpst`, `rubar`, `rumenu`, and `shot`. For example:

```sh
nix build .#rubar
nix build .#rumenu
```

The default Nix package is `rubar`. When building outside Nix on Linux, install a recent Rust toolchain, `pkg-config`, and development libraries for PulseAudio, XCB, Wayland, and xkbcommon. Desktop applications need a compatible graphical session to run. See each application's README for setup, configuration, and platform requirements.

## Development

Run the workspace checks from the repository root:

```sh
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
```

## License

Licensed under the MIT License. See [LICENSE.txt](LICENSE.txt).
