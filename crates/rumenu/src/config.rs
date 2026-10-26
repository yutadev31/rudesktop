use std::{error::Error, fs, path::PathBuf};

use serde::Deserialize;

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub width: u32,
    pub height: u32,
    pub max_results: usize,
    pub font_family: String,
    pub font_size: f32,
    pub icon_theme: String,
    pub terminal: Vec<String>,
    pub background: String,
    pub foreground: String,
    pub selection: String,
    pub muted: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width: 560,
            height: 420,
            max_results: 8,
            font_family: "sans-serif".into(),
            font_size: 16.0,
            icon_theme: "hicolor".into(),
            terminal: Vec::new(),
            background: "#24283b".into(),
            foreground: "#c0caf5".into(),
            selection: "#414868".into(),
            muted: "#9aa5ce".into(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn Error>> {
        let path = config_path()?;
        let config = match fs::read_to_string(&path) {
            Ok(contents) => toml::from_str(&contents)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(error) => return Err(error.into()),
        };
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        if !(200..=4096).contains(&self.width)
            || !(110..=2160).contains(&self.height)
            || !(1..=50).contains(&self.max_results)
            || !(8.0..=64.0).contains(&self.font_size)
        {
            return Err("invalid rumenu size, result count, or font size".into());
        }
        for color in [
            &self.background,
            &self.foreground,
            &self.selection,
            &self.muted,
        ] {
            if !color.starts_with('#')
                || color.len() != 7
                || !color[1..].bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err(format!("invalid color: {color}").into());
            }
        }
        Ok(())
    }
}

fn config_path() -> Result<PathBuf, Box<dyn Error>> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(value) if PathBuf::from(&value).is_absolute() => PathBuf::from(value),
        _ => dirs::home_dir()
            .ok_or("home directory is unavailable")?
            .join(".config"),
    };
    Ok(base.join("rumenu/config.toml"))
}
