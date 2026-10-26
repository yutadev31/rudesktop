use std::{error::Error, io, sync::Arc};

use smithay::{
    delegate_compositor,
    reexports::{
        calloop::{EventLoop, Interest, Mode, PostAction, generic::Generic},
        wayland_server::{
            Client, Display, DisplayHandle,
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
        },
    },
    wayland::{
        compositor::{CompositorClientState, CompositorHandler, CompositorState},
        socket::ListeningSocketSource,
    },
};

struct Ruwm {
    compositor_state: CompositorState,
}

impl CompositorHandler for Ruwm {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client
            .get_data::<RuwmClientState>()
            .expect("ruwm clients have compositor state")
            .compositor_state
    }

    fn commit(&mut self, _surface: &WlSurface) {
        // Rendering and surface management will be added with an output backend.
    }
}

delegate_compositor!(Ruwm);

#[derive(Default)]
struct RuwmClientState {
    compositor_state: CompositorClientState,
}

impl ClientData for RuwmClientState {
    fn initialized(&self, _client_id: ClientId) {}

    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

struct LoopData {
    compositor: Ruwm,
    display_handle: DisplayHandle,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut event_loop: EventLoop<LoopData> = EventLoop::try_new()?;
    let display: Display<Ruwm> = Display::new()?;
    let display_handle = display.handle();
    let compositor = Ruwm {
        compositor_state: CompositorState::new::<Ruwm>(&display_handle),
    };

    let socket = ListeningSocketSource::new_auto()?;
    let socket_name = socket.socket_name().to_os_string();
    let loop_handle = event_loop.handle();

    loop_handle.insert_source(socket, |stream, _, data| {
        if let Err(error) = data
            .display_handle
            .insert_client(stream, Arc::new(RuwmClientState::default()))
        {
            eprintln!("ruwm: failed to accept Wayland client: {error}");
        }
    })?;

    loop_handle.insert_source(
        Generic::new(display, Interest::READ, Mode::Level),
        |_, display, data| {
            // SAFETY: the display remains inside this event source for its entire lifetime.
            unsafe { display.get_mut() }
                .dispatch_clients(&mut data.compositor)
                .map_err(io::Error::other)?;
            Ok(PostAction::Continue)
        },
    )?;

    let mut data = LoopData {
        compositor,
        display_handle,
    };
    eprintln!("ruwm: listening on {}", socket_name.to_string_lossy());

    loop {
        event_loop.dispatch(None, &mut data)?;
        data.display_handle.flush_clients()?;
    }
}
