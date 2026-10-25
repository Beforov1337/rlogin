use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionType {
    Wayland,
    X11,
    Shell,
}

#[derive(Debug, Clone)]
pub struct SessionEntry {
    pub name: String,
    pub exec: String,
    pub session_type: SessionType,
    pub desktop_names: Option<String>,
}

pub fn detect_sessions() -> Vec<SessionEntry> {
    let mut sessions = Vec::new();

    // Check Wayland sessions
    let wayland_dirs = ["/usr/share/wayland-sessions", "/usr/local/share/wayland-sessions"];
    for dir in &wayland_dirs {
        scan_desktop_dir(Path::new(dir), SessionType::Wayland, &mut sessions);
    }

    // Check X11 sessions
    let x11_dirs = ["/usr/share/xsessions", "/usr/local/share/xsessions"];
    for dir in &x11_dirs {
        scan_desktop_dir(Path::new(dir), SessionType::X11, &mut sessions);
    }

    // Default Fallback: Shell session
    sessions.push(SessionEntry {
        name: "Default Shell".to_string(),
        exec: "/bin/sh -l".to_string(),
        session_type: SessionType::Shell,
        desktop_names: None,
    });

    sessions
}

fn scan_desktop_dir(dir: &Path, s_type: SessionType, list: &mut Vec<SessionEntry>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("desktop")
                && let Some(session) = parse_desktop_file(&path, s_type.clone()) {
                    list.push(session);
                }
        }
    }
}

fn parse_desktop_file(path: &PathBuf, s_type: SessionType) -> Option<SessionEntry> {
    let content = fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut exec = None;
    let mut desktop_names = None;

    let mut in_desktop_entry = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_desktop_entry = trimmed == "[Desktop Entry]";
            continue;
        }

        if !in_desktop_entry {
            continue;
        }

        if let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "Name" if name.is_none() => name = Some(val.to_string()),
                "Exec" if exec.is_none() => exec = Some(val.to_string()),
                "DesktopNames" if desktop_names.is_none() => desktop_names = Some(val.to_string()),
                _ => {}
            }
        }
    }

    let session_name = name.unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string()
    });

    let exec_cmd = exec?;

    Some(SessionEntry {
        name: session_name,
        exec: exec_cmd,
        session_type: s_type,
        desktop_names,
    })
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desktop_file_parser() {
        let sample = r#"
[Desktop Entry]
Name=Sway
Comment=SirCmpwn's Wayland window manager
Exec=sway
Type=Application
DesktopNames=sway
"#;
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("sway_test.desktop");
        std::fs::write(&file_path, sample).unwrap();

        let parsed = parse_desktop_file(&file_path, SessionType::Wayland);
        assert!(parsed.is_some());
        let session = parsed.unwrap();
        assert_eq!(session.name, "Sway");
        assert_eq!(session.exec, "sway");
        assert_eq!(session.session_type, SessionType::Wayland);
        assert_eq!(session.desktop_names.as_deref(), Some("sway"));

        let _ = std::fs::remove_file(file_path);
    }
}
