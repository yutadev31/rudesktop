use std::{
    collections::{HashMap, HashSet},
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use cosmic_text::{
    Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap,
};
use shell_surface::Size;
use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::{catalog::Entry, config::Config};

const ICON_SIZE: u32 = 32;

pub struct Renderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    config: Config,
    icons: HashMap<String, Option<Vec<u8>>>,
}

impl Renderer {
    pub fn new(config: &Config) -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            config: config.clone(),
            icons: HashMap::new(),
        }
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
                let pixels = self
                    .icons
                    .entry(icon.clone())
                    .or_insert_with(|| load_icon(icon, &self.config.icon_theme));
                if let Some(pixels) = pixels {
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

fn icon_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    {
        roots.push(home.join("icons"));
    }
    let system = env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    roots.extend(env::split_paths(&system).map(|dir| dir.join("icons")));
    roots
}

fn load_icon(name: &str, theme: &str) -> Option<Vec<u8>> {
    let path = Path::new(name);
    if path.is_absolute() {
        return decode_icon(path);
    }
    let roots = icon_roots();
    let mut visited = HashSet::new();
    for theme in [theme, "hicolor"] {
        if let Some(path) = find_theme_icon(&roots, theme, name, &mut visited)
            && let Some(pixels) = decode_icon(&path)
        {
            return Some(pixels);
        }
    }
    for root in roots {
        for ext in ["png", "svg"] {
            if let Some(pixels) = decode_icon(&root.join(format!("{name}.{ext}"))) {
                return Some(pixels);
            }
        }
    }
    for ext in ["png", "svg"] {
        if let Some(pixels) =
            decode_icon(&PathBuf::from(format!("/usr/share/pixmaps/{name}.{ext}")))
        {
            return Some(pixels);
        }
    }
    None
}

fn find_theme_icon(
    roots: &[PathBuf],
    theme: &str,
    name: &str,
    visited: &mut HashSet<String>,
) -> Option<PathBuf> {
    if !visited.insert(theme.to_string()) {
        return None;
    }
    let mut parents = Vec::new();
    for root in roots {
        let theme_dir = root.join(theme);
        let Ok(index) = fs::read_to_string(theme_dir.join("index.theme")) else {
            continue;
        };
        let mut directories = Vec::new();
        for line in index.lines() {
            if let Some(value) = line.strip_prefix("Directories=") {
                directories.extend(value.split(',').map(str::trim).map(str::to_string));
            } else if let Some(value) = line.strip_prefix("Inherits=") {
                parents.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(str::to_string),
                );
            }
        }
        directories.sort_by_key(|dir| icon_directory_distance(dir));
        for dir in directories {
            for ext in ["png", "svg"] {
                let path = theme_dir.join(&dir).join(format!("{name}.{ext}"));
                if path.is_file() {
                    return Some(path);
                }
            }
        }
    }
    for parent in parents {
        if let Some(icon) = find_theme_icon(roots, &parent, name, visited) {
            return Some(icon);
        }
    }
    None
}

fn icon_directory_distance(path: &str) -> u32 {
    if path.starts_with("scalable/") {
        return 1;
    }
    let Some((size, _)) = path.split_once('x') else {
        return u32::MAX;
    };
    size.parse::<u32>()
        .map_or(u32::MAX, |size| size.abs_diff(ICON_SIZE))
}

fn decode_icon(path: &Path) -> Option<Vec<u8>> {
    if path.extension()?.to_str()? == "svg" {
        let data = fs::read(path).ok()?;
        let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).ok()?;
        let mut pixmap = Pixmap::new(ICON_SIZE, ICON_SIZE)?;
        let scale =
            (ICON_SIZE as f32 / tree.size().width()).min(ICON_SIZE as f32 / tree.size().height());
        resvg::render(
            &tree,
            Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        let mut pixels = pixmap.take();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let alpha = u16::from(pixel[3]).max(1);
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * 255) / alpha).min(255) as u8;
            }
        }
        Some(pixels)
    } else {
        let image = image::open(path)
            .ok()?
            .resize_exact(ICON_SIZE, ICON_SIZE, image::imageops::FilterType::Lanczos3)
            .into_rgba8();
        Some(image.into_raw())
    }
}

fn draw_icon(pixmap: &mut Pixmap, icon: &[u8], x: i32, y: i32) {
    if icon.len() != (ICON_SIZE * ICON_SIZE * 4) as usize {
        return;
    }
    for row in 0..ICON_SIZE as i32 {
        for col in 0..ICON_SIZE as i32 {
            let px = x + col;
            let py = y + row;
            if px < 0 || py < 0 || px >= pixmap.width() as i32 || py >= pixmap.height() as i32 {
                continue;
            }
            let source = ((row as u32 * ICON_SIZE + col as u32) * 4) as usize;
            let target = ((py as u32 * pixmap.width() + px as u32) * 4) as usize;
            let alpha = icon[source + 3] as u16;
            let data = pixmap.data_mut();
            for channel in 0..3 {
                data[target + channel] = ((icon[source + channel] as u16 * alpha
                    + data[target + channel] as u16 * (255 - alpha))
                    / 255) as u8;
            }
            data[target + 3] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_up_inherited_icon_theme() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("icons");
        let child = root.join("child");
        let parent = root.join("parent");
        fs::create_dir_all(parent.join("32x32/apps")).unwrap();
        fs::create_dir_all(&child).unwrap();
        fs::write(
            child.join("index.theme"),
            "[Icon Theme]\nInherits=parent\nDirectories=32x32/apps\n",
        )
        .unwrap();
        fs::write(
            parent.join("index.theme"),
            "[Icon Theme]\nDirectories=32x32/apps\n",
        )
        .unwrap();
        let icon = parent.join("32x32/apps/example.png");
        fs::write(&icon, b"test").unwrap();
        assert_eq!(
            find_theme_icon(&[root], "child", "example", &mut HashSet::new()),
            Some(icon)
        );
    }
}
