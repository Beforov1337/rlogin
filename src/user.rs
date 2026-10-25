use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct UserEntry {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub dir: String,
    pub shell: String,
}

pub fn detect_users() -> Vec<UserEntry> {
    let mut users = Vec::new();
    let passwd_path = Path::new("/etc/passwd");

    let Ok(file) = File::open(passwd_path) else {
        return users;
    };

    let reader = BufReader::new(file);

    for line in reader.lines().map_while(Result::ok) {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 7 {
            let name = parts[0];
            let uid: u32 = match parts[2].parse() {
                Ok(u) => u,
                Err(_) => continue,
            };
            let gid: u32 = match parts[3].parse() {
                Ok(g) => g,
                Err(_) => continue,
            };
            let dir = parts[5];
            let shell = parts[6];

            // Filter system users: typically UIDs >= 1000, and exclude nologin/false/nobody
            let valid_shell = !shell.ends_with("/nologin") && !shell.ends_with("/false");
            let is_interactive = (1000..65534).contains(&uid) && valid_shell;

            if is_interactive {
                users.push(UserEntry {
                    name: name.to_string(),
                    uid,
                    gid,
                    dir: dir.to_string(),
                    shell: shell.to_string(),
                });
            }
        }
    }

    // Sort alphabetically
    users.sort_by(|a, b| a.name.cmp(&b.name));
    users
}
