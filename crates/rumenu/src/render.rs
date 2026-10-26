use std::{
    collections::{HashMap, HashSet},
    error::Error,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
};

use cosmic_text::{
    Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap,
};
use desktop_icons::{Icon, Loader, RasterSize};
use shell_surface::Size;
use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::{catalog::Entry, config::Config};

const ICON_SIZE: u32 = 32;

pub struct Renderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    config: Config,
    icons: HashMap<String, Option<Icon>>,
    pending_icons: HashSet<String>,
    icon_requests: Sender<String>,
    icon_results: Receiver<(String, Option<Icon>)>,
    icon_worker_failed: bool,
}

impl Renderer {
    pub fn new(config: &Config) -> Result<Self, Box<dyn Error>> {
        let (icon_requests, requests) = mpsc::channel::<String>();
        let (results, icon_results) = mpsc::channel();
        let theme = config.icon_theme.clone();
        thread::Builder::new()
            .name("rumenu-icons".into())
            .spawn(move || {
                let loader = Loader::new([theme]);
                while let Ok(name) = requests.recv() {
                    let icon = loader.load(&name, ICON_SIZE, RasterSize::Target);
                    if results.send((name, icon)).is_err() {
                        break;
                    }
                }
            })?;

        Ok(Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            config: config.clone(),
            icons: HashMap::new(),
            pending_icons: HashSet::new(),
            icon_requests,
            icon_results,
            icon_worker_failed: false,
        })
    }

    pub fn collect_icons(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.icon_results.try_recv() {
                Ok((name, icon)) => {
                    self.pending_icons.remove(&name);
                    self.icons.insert(name, icon);
                    changed = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !self.icon_worker_failed && !self.pending_icons.is_empty() {
                        tracing::error!("icon loader stopped before completing pending requests");
                        self.icon_worker_failed = true;
                    }

                    break;
                }
            }
        }
        changed
    }

    pub fn render(
        &mut self,
        size: Size,
        query: &str,
        entries: &[&Entry],
        selected: usize,
        error: Option<&str>,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut pixmap = Pixmap::new(size.width, size.height).ok_or("invalid render size")?;
        pixmap.fill(color(&self.config.background)?);
        fill_rect(
            &mut pixmap,
            12.0,
            12.0,
            size.width.saturating_sub(24) as f32,
            42.0,
            color(&self.config.selection)?,
        );

        self.text(
            &mut pixmap,
            if query.is_empty() {
                "Search applications…"
            } else {
                query
            },
            24,
            19,
            self.config.font_size + 2.0,
            &self.config.foreground.clone(),
        );
        for (row, entry) in entries.iter().enumerate() {
            let y = 66 + row as i32 * 44;

            if y + 44 > size.height as i32 {
                break;
            }
            if row == selected {
                fill_rect(
                    &mut pixmap,
                    12.0,
                    y as f32,
                    size.width.saturating_sub(24) as f32,
                    42.0,
                    color(&self.config.selection)?,
                );
            }

            if let Some(icon) = &entry.icon {
                if !self.icons.contains_key(icon)
                    && self.pending_icons.insert(icon.clone())
                    && self.icon_requests.send(icon.clone()).is_err()
                {
                    if !self.icon_worker_failed {
                        tracing::error!("icon loader stopped before accepting requests");
                        self.icon_worker_failed = true;
                    }
                    self.pending_icons.remove(icon);
                    self.icons.insert(icon.clone(), None);
                }
                if let Some(Some(pixels)) = self.icons.get(icon) {
                    draw_icon(&mut pixmap, pixels, 22, y + 5);
                }
            }

            self.text(
                &mut pixmap,
                &entry.name,
                64,
                y + 3,
                self.config.font_size,
                &self.config.foreground.clone(),
            );
            if !entry.comment.is_empty() {
                self.text(
                    &mut pixmap,
                    &entry.comment,
                    64,
                    y + 23,
                    self.config.font_size * 0.75,
                    &self.config.muted.clone(),
                );
            }
        }

        if let Some(error) = error {
            self.text(
                &mut pixmap,
                error,
                20,
                size.height as i32 - 25,
                self.config.font_size * 0.8,
                "#ff6677",
            );
        }
        let mut bytes = pixmap.take();
        for pixel in bytes.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        Ok(bytes)
    }

    fn text(
        &mut self,
        pixmap: &mut Pixmap,
        value: &str,
        x: i32,
        y: i32,
        font_size: f32,
        color_value: &str,
    ) {
        let Ok(rgba) = color(color_value) else { return };
        let text_color = TextColor::rgba(
            (rgba.red() * 255.0) as u8,
            (rgba.green() * 255.0) as u8,
            (rgba.blue() * 255.0) as u8,
            255,
        );

        let mut buffer = Buffer::new(
            &mut self.font_system,
            Metrics::new(font_size, font_size * 1.25),
        );
        buffer.set_size(
            Some((pixmap.width() as i32 - x - 18).max(1) as f32),
            Some(font_size * 1.4),
        );
        buffer.set_wrap(Wrap::None);
        let attrs = Attrs::new()
            .family(Family::Name(&self.config.font_family))
            .color(text_color);
        buffer.set_text(value, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            text_color,
            |gx, gy, _, _, glyph_color| {
                let px = x + gx;
                let py = y + gy;
                if px < 0 || py < 0 || px >= pixmap.width() as i32 || py >= pixmap.height() as i32 {
                    return;
                }
                let offset = (py as usize * pixmap.width() as usize + px as usize) * 4;
                let [sr, sg, sb, alpha] = glyph_color.as_rgba();
                let data = pixmap.data_mut();
                let inverse = 255 - alpha as u16;

                for (i, source) in [sr, sg, sb].into_iter().enumerate() {
                    data[offset + i] = ((u16::from(source) * u16::from(alpha)
                        + u16::from(data[offset + i]) * inverse)
                        / 255) as u8;
                }
                data[offset + 3] = 255;
            },
        );
    }
}

fn color(value: &str) -> Result<tiny_skia::Color, Box<dyn Error>> {
    let red = u8::from_str_radix(&value[1..3], 16)?;
    let green = u8::from_str_radix(&value[3..5], 16)?;
    let blue = u8::from_str_radix(&value[5..7], 16)?;
    Ok(tiny_skia::Color::from_rgba8(red, green, blue, 255))
}

fn fill_rect(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: tiny_skia::Color,
) {
    if let Some(rect) = Rect::from_xywh(x, y, width, height) {
        let mut paint = Paint::default();
        paint.set_color(color);
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

fn draw_icon(pixmap: &mut Pixmap, icon: &Icon, x: i32, y: i32) {
    if icon.pixels.len()
        != (icon.width as usize)
            .saturating_mul(icon.height as usize)
            .saturating_mul(4)
    {
        return;
    }
    for row in 0..icon.height as i32 {
        for col in 0..icon.width as i32 {
            let px = x + col;
            let py = y + row;
            if px < 0 || py < 0 || px >= pixmap.width() as i32 || py >= pixmap.height() as i32 {
                continue;
            }

            let source = ((row as u32 * icon.width + col as u32) * 4) as usize;
            let target = ((py as u32 * pixmap.width() + px as u32) * 4) as usize;
            let alpha = icon.pixels[source + 3] as u16;
            let data = pixmap.data_mut();
            for channel in 0..3 {
                data[target + channel] = (icon.pixels[source + channel] as u16
                    + data[target + channel] as u16 * (255 - alpha) / 255)
                    .min(255) as u8;
            }
            data[target + 3] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{Duration, Instant},
    };

    #[test]
    fn blends_premultiplied_icon_pixels() {
        let mut pixmap = Pixmap::new(1, 1).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(0, 0, 255, 255));
        draw_icon(
            &mut pixmap,
            &Icon {
                width: 1,
                height: 1,
                pixels: vec![128, 0, 0, 128],
            },
            0,
            0,
        );
        assert_eq!(pixmap.data()[..4], [128, 0, 127, 255]);
    }

    #[test]
    fn loads_icons_after_the_first_render() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("example.svg");
        fs::write(
            &path,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#ff0000"/></svg>"##,
        )
        .unwrap();
        let entry = Entry {
            name: "Example".into(),
            comment: String::new(),
            keywords: String::new(),
            icon: Some(path.to_string_lossy().into_owned()),
            path: directory.path().join("example.desktop"),

            exec: "example".into(),
            terminal: false,
            working_dir: None,
        };
        let config = Config::default();
        let mut renderer = Renderer::new(&config).unwrap();
        renderer
            .render(
                Size::new(config.width, config.height),
                "",
                &[&entry],
                0,
                None,
            )
            .unwrap();
        assert!(renderer.icons.is_empty());
        assert_eq!(renderer.pending_icons.len(), 1);

        let deadline = Instant::now() + Duration::from_secs(5);
        while !renderer.collect_icons() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            renderer
                .icons
                .get(entry.icon.as_ref().unwrap())
                .unwrap()
                .is_some()
        );
        assert!(renderer.pending_icons.is_empty());
    }
}
