use std::{
    collections::{HashMap, HashSet},
    env,
    error::Error,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Clone)]
pub struct Entry {
    pub name: String,
    pub comment: String,
    pub keywords: String,
    pub icon: Option<String>,
    pub path: PathBuf,
    pub exec: String,
    pub terminal: bool,
    pub working_dir: Option<PathBuf>,
}

pub fn scan(terminal_available: bool) -> Vec<Entry> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for base in data_dirs() {
        visit(
            &base.join("applications"),
            &base.join("applications"),
            &mut seen,
            &mut entries,
            terminal_available,
        );
    }
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    entries
}

fn data_dirs() -> Vec<PathBuf> {
    let home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let mut dirs = home.into_iter().collect::<Vec<_>>();
    let system = env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(env::split_paths(&system).filter(|path| path.is_absolute()));
    dirs
}

fn visit(
    root: &Path,
    dir: &Path,
    seen: &mut HashSet<String>,
    entries: &mut Vec<Entry>,
    terminal_available: bool,
) {
    let read_dir = match fs::read_dir(dir) {
        Ok(read_dir) => read_dir,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!(path = %dir.display(), %error, "failed to read application directory");
            return;
        }
    };
    for item in read_dir {
        let item = match item {
            Ok(item) => item,
            Err(error) => {
                tracing::warn!(path = %dir.display(), %error, "failed to read application directory entry");
                continue;
            }
        };
        let path = item.path();
        let file_type = match item.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "failed to read application entry type");
                continue;
            }
        };
        if file_type.is_dir() {
            visit(root, &path, seen, entries, terminal_available);
        } else if (file_type.is_file() || file_type.is_symlink())
            && path.extension().is_some_and(|ext| ext == "desktop")
        {
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let id = relative.to_string_lossy().replace('/', "-");
            if !seen.insert(id) {
                continue;
            }
            match parse(&path, terminal_available) {
                Ok(Some(entry)) => entries.push(entry),
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(path = %path.display(), %error, "invalid desktop entry")
                }
            }
        }
    }
}

fn parse(path: &Path, terminal_available: bool) -> Result<Option<Entry>, Box<dyn Error>> {
    let contents = fs::read_to_string(path)?;
    let mut fields = HashMap::new();
    let mut in_desktop_entry = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
        } else if in_desktop_entry
            && !line.starts_with('#')
            && let Some((key, value)) = line.split_once('=')
        {
            fields.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    if fields.get("Type").map(String::as_str) != Some("Application")
        || matches!(fields.get("Hidden").map(String::as_str), Some("true"))
        || matches!(fields.get("NoDisplay").map(String::as_str), Some("true"))
    {
        return Ok(None);
    }
    let desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let desktops = desktop.split(':').collect::<Vec<_>>();
    if fields
        .get("OnlyShowIn")
        .is_some_and(|v| !list(v).iter().any(|name| desktops.contains(name)))
        || fields
            .get("NotShowIn")
            .is_some_and(|v| list(v).iter().any(|name| desktops.contains(name)))
    {
        return Ok(None);
    }
    if fields
        .get("TryExec")
        .is_some_and(|exe| !executable_exists(exe))
    {
        return Ok(None);
    }
    let terminal = fields.get("Terminal").is_some_and(|v| v == "true");
    if terminal && !terminal_available {
        return Ok(None);
    }
    let Some(name) = localized(&fields, "Name") else {
        return Ok(None);
    };
    let Some(exec) = fields.get("Exec") else {
        return Ok(None);
    };
    Ok(Some(Entry {
        name,
        comment: localized(&fields, "Comment").unwrap_or_default(),
        keywords: localized(&fields, "Keywords").unwrap_or_default(),
        icon: fields.get("Icon").cloned(),
        path: path.to_path_buf(),
        exec: exec.clone(),
        terminal,
        working_dir: fields.get("Path").map(PathBuf::from),
    }))
}

fn list(value: &str) -> Vec<&str> {
    value.split(';').filter(|v| !v.is_empty()).collect()
}

fn localized(fields: &HashMap<String, String>, key: &str) -> Option<String> {
    let locale = env::var("LC_ALL")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| env::var("LC_MESSAGES").ok().filter(|v| !v.is_empty()))
        .or_else(|| env::var("LANG").ok())
        .unwrap_or_default();
    let locale = locale.split('.').next().unwrap_or("");
    let language = locale.split('_').next().unwrap_or("");
    [
        format!("{key}[{locale}]"),
        format!("{key}[{language}]"),
        key.to_string(),
    ]
    .into_iter()
    .find_map(|candidate| fields.get(&candidate).map(|v| unescape(v)))
}

fn unescape(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            result.push(match chars.next() {
                Some('s') => ' ',
                Some('n') => '\n',
                Some('t') => '\t',
                Some('r') => '\r',
                Some('\\') => '\\',
                Some(';') => ';',
                Some(other) => other,
                None => '\\',
            });
        } else {
            result.push(ch);
        }
    }
    result
}

fn executable_exists(name: &str) -> bool {
    let is_executable = |path: &Path| {
        fs::metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    };
    let candidate = Path::new(name);
    if candidate.components().count() > 1 {
        return is_executable(candidate);
    }
    env::var_os("PATH")
        .is_some_and(|path| env::split_paths(&path).any(|dir| is_executable(&dir.join(name))))
}

pub fn matches(entry: &Entry, query: &str) -> bool {
    let query = query.to_lowercase();
    entry.name.to_lowercase().contains(&query)
        || entry.comment.to_lowercase().contains(&query)
        || entry.keywords.to_lowercase().contains(&query)
}

pub fn launch(entry: &Entry, terminal: &[String]) -> Result<(), Box<dyn Error>> {
    let args = expand_exec(entry)?;
    let (program, arguments) = args
        .split_first()
        .ok_or("desktop entry has an empty Exec")?;
    let mut command = if entry.terminal {
        let (terminal_program, terminal_args) =
            terminal.split_first().ok_or("terminal is not configured")?;
        let mut command = Command::new(terminal_program);
        command.args(terminal_args).arg(program);
        command
    } else {
        Command::new(program)
    };
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = &entry.working_dir {
        command.current_dir(dir);
    }
    let mut child = command.spawn()?;
    std::thread::spawn(move || {
        if let Err(error) = child.wait() {
            tracing::warn!(%error, "failed to reap launched application");
        }
    });
    Ok(())
}

fn expand_exec(entry: &Entry) -> Result<Vec<String>, Box<dyn Error>> {
    let mut args = Vec::new();
    for token in tokenize(&entry.exec)? {
        if token == "%i" {
            if let Some(icon) = &entry.icon {
                args.push("--icon".into());
                args.push(icon.clone());
            }
            continue;
        }
        let mut result = String::new();
        let mut chars = token.chars();
        while let Some(ch) = chars.next() {
            if ch != '%' {
                result.push(ch);
                continue;
            }
            match chars.next().ok_or("trailing % in Exec")? {
                '%' => result.push('%'),
                'c' => result.push_str(&entry.name),
                'k' => result.push_str(&entry.path.to_string_lossy()),
                'f' | 'F' | 'u' | 'U' | 'd' | 'D' | 'n' | 'N' | 'v' | 'm' => {}
                other => return Err(format!("unsupported Exec field code %{other}").into()),
            }
        }
        if !result.is_empty() {
            args.push(result);
        }
    }
    Ok(args)
}

fn tokenize(value: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let mut args = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            token.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            quoted = !quoted;
        } else if ch.is_whitespace() && !quoted {
            if !token.is_empty() {
                args.push(std::mem::take(&mut token));
            }
        } else {
            token.push(ch);
        }
    }
    if quoted || escaped {
        return Err("unterminated Exec quotation".into());
    }
    if !token.is_empty() {
        args.push(token);
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    fn desktop(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            format!("[Desktop Entry]\nType=Application\nName=Example\nExec=example\n{body}"),
        )
        .unwrap();
    }

    #[test]
    fn exec_preserves_argument_boundaries() {
        let entry = Entry {
            name: "Two Words".into(),
            comment: String::new(),
            keywords: String::new(),
            icon: Some("icon".into()),
            path: PathBuf::from("/tmp/app.desktop"),
            exec: "program --title %c %i \"two words\" %%".into(),
            terminal: false,
            working_dir: None,
        };
        assert_eq!(
            expand_exec(&entry).unwrap(),
            [
                "program",
                "--title",
                "Two Words",
                "--icon",
                "icon",
                "two words",
                "%"
            ]
        );
    }

    #[test]
    fn hidden_user_entry_masks_system_entry() {
        let directory = tempdir().unwrap();
        let user = directory.path().join("user/applications");
        let system = directory.path().join("system/applications");
        desktop(&user.join("example.desktop"), "Hidden=true\n");
        desktop(&system.join("example.desktop"), "");
        let mut seen = HashSet::new();
        let mut entries = Vec::new();
        visit(&user, &user, &mut seen, &mut entries, false);
        visit(&system, &system, &mut seen, &mut entries, false);
        assert!(entries.is_empty());
    }

    #[test]
    fn visits_desktop_symlinks_without_following_directory_symlinks() {
        let directory = tempdir().unwrap();
        let root = directory.path().join("applications");
        let external = directory.path().join("external");
        desktop(&external.join("example.desktop"), "");
        fs::create_dir(&root).unwrap();
        symlink(
            external.join("example.desktop"),
            root.join("linked.desktop"),
        )
        .unwrap();
        symlink(&external, root.join("linked-directory")).unwrap();

        let mut seen = HashSet::new();
        let mut entries = Vec::new();
        visit(&root, &root, &mut seen, &mut entries, false);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, root.join("linked.desktop"));
    }

    #[test]
    fn skips_terminal_without_configuration_and_filters_matches() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("example.desktop");
        desktop(
            &path,
            "Terminal=true\nComment=Web browser\nKeywords=internet;pages;\n",
        );
        assert!(parse(&path, false).unwrap().is_none());
        let entry = parse(&path, true).unwrap().unwrap();
        assert!(matches(&entry, "BROWSER"));
        assert!(matches(&entry, "PAGES"));
        assert!(!matches(&entry, "terminal"));
    }

    #[test]
    fn rejects_unknown_exec_field_code() {
        let entry = Entry {
            name: "Example".into(),
            comment: String::new(),
            keywords: String::new(),
            icon: None,
            path: PathBuf::from("/tmp/example.desktop"),
            exec: "app %Z".into(),
            terminal: false,
            working_dir: None,
        };
        assert!(expand_exec(&entry).is_err());
    }
}
