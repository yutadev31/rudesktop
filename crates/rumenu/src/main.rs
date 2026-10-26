mod app;
mod catalog;
mod config;
mod render;

use std::{
    error::Error,
    io::{Read, Write},
    os::unix::fs::FileTypeExt,
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let socket = socket_path()?;
    match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => serve(socket),
        [command] if command == "toggle" => {
            let mut stream = UnixStream::connect(&socket)
                .map_err(|error| format!("rumenu is not running: {error}"))?;
            stream.write_all(b"toggle\n")?;
            let mut response = [0; 1];
            stream.read_exact(&mut response)?;
            Ok(())
        }
        _ => Err("usage: rumenu [toggle]".into()),
    }
}

fn socket_path() -> Result<PathBuf, Box<dyn Error>> {
    let runtime =
        PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is required")?);
    if !runtime.is_absolute() {
        return Err("XDG_RUNTIME_DIR must be absolute".into());
    }
    let display = std::env::var("WAYLAND_DISPLAY").or_else(|_| std::env::var("DISPLAY"))?;
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in display.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    Ok(runtime.join(format!("rumenu-{hash:016x}.sock")))
}

fn serve(socket: PathBuf) -> Result<(), Box<dyn Error>> {
    if let Ok(metadata) = std::fs::symlink_metadata(&socket) {
        if !metadata.file_type().is_socket() {
            return Err(format!("socket path is occupied: {}", socket.display()).into());
        }
        if UnixStream::connect(&socket).is_ok() {
            return Err("rumenu is already running on this display".into());
        }
        std::fs::remove_file(&socket)?;
    }
    let listener = UnixListener::bind(&socket)?;
    let _socket_guard = SocketGuard(socket);
    listener.set_nonblocking(true)?;
    let mut ui: Option<(Arc<AtomicBool>, thread::JoinHandle<()>)> = None;

    loop {
        if ui.as_ref().is_some_and(|(_, handle)| handle.is_finished())
            && let Some((_, handle)) = ui.take()
        {
            let _ = handle.join();
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_secs(1)))?;
                let mut request = [0; 7];
                if stream.read_exact(&mut request).is_ok() && &request == b"toggle\n" {
                    toggle_ui(&mut ui);
                    stream.write_all(b"\n")?;
                }
            }

            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
        thread::sleep(Duration::from_millis(30));
    }
}

fn toggle_ui(ui: &mut Option<(Arc<AtomicBool>, thread::JoinHandle<()>)>) {
    if let Some((close, handle)) = ui.take() {
        close.store(true, Ordering::Release);
        let _ = handle.join();
    } else {
        let close = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&close);
        let handle = thread::spawn(move || {
            if let Err(error) = app::run(signal) {
                tracing::error!(%error, "launcher failed");
            }
        });
        *ui = Some((close, handle));
    }
}

struct SocketGuard(PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.0) {
            tracing::warn!(path = %self.0.display(), %error, "could not remove IPC socket");
        }
    }
}
