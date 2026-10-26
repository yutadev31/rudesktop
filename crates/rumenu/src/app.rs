use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use shell_surface::{
    Backend, InputEvent, Key, KeyboardInteractivity, Layer, MouseButton, Shell, Size,
    SurfaceConfig, SurfaceId,
};
use x11rb::connection::Connection;

use crate::{
    catalog::{self, Entry},
    config::Config,
    render::Renderer,
};

pub fn run(close: Arc<AtomicBool>) -> Result<(), Box<dyn Error>> {
    let config = Config::load()?;
    config.validate()?;
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let mut surface = SurfaceConfig::new("rumenu", Size::new(config.width, config.height));
    surface.layer = Layer::Overlay;
    surface.keyboard_interactivity = KeyboardInteractivity::Exclusive;
    if !wayland {
        let (connection, screen_number) = x11rb::connect(None)?;
        let screen = &connection.setup().roots[screen_number];
        surface.position.x = (i32::from(screen.width_in_pixels) - config.width as i32) / 2;
        surface.position.y = (i32::from(screen.height_in_pixels) - config.height as i32) / 2;
        surface.override_redirect = true;
    }
    let mut app = App {
        entries: catalog::scan(!config.terminal.is_empty()),
        filtered: Vec::new(),
        query: String::new(),
        selected: 0,
        scroll: 0,
        error: None,
        renderer: Renderer::new(&config),
        config,
        surfaces: vec![surface],
        close,
        redraw: true,
    };
    app.refilter();
    let mut backend: Box<dyn Backend> = if wayland {
        Box::new(shell_surface::backend::wayland::WaylandBackend)
    } else {
        Box::new(shell_surface::backend::x11::X11Backend)
    };
    backend.run(&mut app)
}

struct App {
    entries: Vec<Entry>,
    filtered: Vec<usize>,
    query: String,
    selected: usize,
    scroll: usize,
    error: Option<String>,
    renderer: Renderer,
    config: Config,
    surfaces: Vec<SurfaceConfig>,
    close: Arc<AtomicBool>,
    redraw: bool,
}

impl App {
    fn visible_rows(&self) -> usize {
        ((self.config.height.saturating_sub(66) / 44) as usize)
            .max(1)
            .min(self.config.max_results)
    }

    fn refilter(&mut self) {
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| catalog::matches(entry, &self.query))
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
        self.scroll = 0;
        self.redraw = true;
    }

    fn launch_selected(&mut self) {
        let Some(index) = self.filtered.get(self.selected).copied() else {
            return;
        };
        match catalog::launch(&self.entries[index], &self.config.terminal) {
            Ok(()) => self.close.store(true, Ordering::Release),
            Err(error) => {
                self.error = Some(error.to_string());
                self.redraw = true;
            }
        }
    }

    fn move_selection(&mut self, down: bool) {
        if self.filtered.is_empty() {
            return;
        }
        if down {
            self.selected = (self.selected + 1).min(self.filtered.len() - 1);
        } else {
            self.selected = self.selected.saturating_sub(1);
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
        if self.selected >= self.scroll + self.visible_rows() {
            self.scroll = self.selected + 1 - self.visible_rows();
        }
        self.redraw = true;
    }
}

impl Shell for App {
    fn surface_configs(&self) -> &[SurfaceConfig] {
        &self.surfaces
    }

    fn render(
        &mut self,
        _surface: SurfaceId,
        size: Size,
        _output: Option<&str>,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let visible = self
            .filtered
            .iter()
            .skip(self.scroll)
            .take(self.visible_rows())
            .map(|index| &self.entries[*index])
            .collect::<Vec<_>>();
        self.renderer.render(
            size,
            &self.query,
            &visible,
            self.selected.saturating_sub(self.scroll),
            self.error.as_deref(),
        )
    }

    fn handle_event(&mut self, _surface: SurfaceId, event: InputEvent) {
        match event {
            InputEvent::CloseRequested | InputEvent::FocusLost => {
                self.close.store(true, Ordering::Release)
            }
            InputEvent::KeyInput {
                key: Key::Escape, ..
            } => self.close.store(true, Ordering::Release),
            InputEvent::KeyInput {
                key: Key::Return, ..
            } => self.launch_selected(),
            InputEvent::KeyInput {
                key: Key::Backspace,
                ..
            } => {
                self.query.pop();
                self.error = None;
                self.refilter();
            }
            InputEvent::KeyInput { key: Key::Up, .. } => self.move_selection(false),
            InputEvent::KeyInput { key: Key::Down, .. } => self.move_selection(true),
            InputEvent::KeyInput {
                text: Some(text), ..
            } => {
                self.query.push_str(&text);
                self.error = None;
                self.refilter();
            }
            InputEvent::PointerButton {
                position,
                button: MouseButton::Left,
                pressed: true,
                ..
            } => {
                if position.y >= 66.0 {
                    let row = ((position.y - 66.0) / 44.0) as usize;
                    let selected = self.scroll + row;
                    if selected < self.filtered.len() && row < self.visible_rows() {
                        self.selected = selected;
                        self.launch_selected();
                    }
                }
            }
            InputEvent::PointerScroll { delta_y, .. } => self.move_selection(delta_y > 0.0),
            _ => {}
        }
    }

    fn take_redraw_request(&mut self) -> bool {
        std::mem::take(&mut self.redraw)
    }
    fn should_close(&self) -> bool {
        self.close.load(Ordering::Acquire)
    }
}
