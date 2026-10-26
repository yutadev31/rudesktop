use std::error::Error;

use smithay::{
    backend::{
        renderer::{Color32F, Frame, Renderer, gles::GlesRenderer},
        winit::{self, WinitEvent, WinitEventLoop, WinitGraphicsBackend},
    },
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::{wayland_server::DisplayHandle, winit::window::Window},
    utils::{Rectangle, Transform},
};

use crate::Ruwm;

pub struct WinitBackend {
    graphics: WinitGraphicsBackend<GlesRenderer>,
    output: Output,
    needs_redraw: bool,
    closed: bool,
}

impl WinitBackend {
    pub fn new(display: &DisplayHandle) -> Result<(Self, WinitEventLoop), Box<dyn Error>> {
        let attributes = Window::default_attributes()
            .with_title("ruwm")
            .with_inner_size(smithay::reexports::winit::dpi::LogicalSize::new(
                1280.0, 800.0,
            ));
        let (graphics, events) = winit::init_from_attributes::<GlesRenderer>(attributes)?;

        let output = Output::new(
            "winit".into(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "ruwm".into(),
                model: "winit".into(),
            },
        );
        let mode = Mode {
            size: graphics.window_size(),
            refresh: 60_000,
        };
        output.create_global::<Ruwm>(display);
        output.change_current_state(
            Some(mode),
            Some(Transform::Flipped180),
            Some(Scale::Integer(1)),
            Some((0, 0).into()),
        );
        output.set_preferred(mode);

        Ok((
            Self {
                graphics,
                output,
                needs_redraw: true,
                closed: false,
            },
            events,
        ))
    }

    pub fn handle_event(&mut self, event: WinitEvent) {
        match event {
            WinitEvent::Resized { size, .. } => {
                self.output.change_current_state(
                    Some(Mode {
                        size,
                        refresh: 60_000,
                    }),
                    None,
                    None,
                    None,
                );
                self.needs_redraw = true;
            }
            WinitEvent::Redraw => self.needs_redraw = true,
            WinitEvent::CloseRequested => self.closed = true,
            _ => {}
        }
    }

    pub fn render_if_needed(&mut self) -> Result<(), Box<dyn Error>> {
        if !self.needs_redraw || self.closed {
            return Ok(());
        }

        let size = self.graphics.window_size();
        if size.w <= 0 || size.h <= 0 {
            return Ok(());
        }

        let damage = Rectangle::from_size(size);
        {
            let (renderer, mut framebuffer) = self.graphics.bind()?;
            let mut frame = renderer.render(&mut framebuffer, size, Transform::Flipped180)?;
            frame.clear(Color32F::new(0.08, 0.09, 0.11, 1.0), &[damage])?;
            let _ = frame.finish()?;
        }
        self.graphics.submit(Some(&[damage]))?;
        self.needs_redraw = false;
        Ok(())
    }

    pub fn cleanup_output(&self) {
        self.output.cleanup();
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }
}
