use pam::Client;
use std::env;
use std::ffi::CString;
use std::os::unix::process::CommandExt;
use std::process::Command;

use crate::session::{SessionEntry, SessionType};
use crate::user::UserEntry;

pub fn authenticate(service_name: &str, username: &str, password: &str) -> Result<(), String> {
    let mut client = Client::with_password(service_name)
        .map_err(|e| format!("Failed to initialize PAM: {:?}", e))?;

    client.conversation_mut().set_credentials(username, password);

    client.authenticate()
        .map_err(|e| format!("Authentication failed: {:?}", e))?;

    client.open_session()
        .map_err(|e| format!("Failed to open PAM session: {:?}", e))?;

    Ok(())
}

pub fn spawn_session(user: &UserEntry, session: &SessionEntry, tty_num: Option<u32>) -> Result<(), std::io::Error> {
    let tty = tty_num.unwrap_or(2);
    let vt_str = format!("vt{}", tty);

    // Prepare environment variables
    let mut envs: Vec<(String, String)> = vec![
        ("USER".into(), user.name.clone()),
        ("LOGNAME".into(), user.name.clone()),
        ("HOME".into(), user.dir.clone()),
        ("SHELL".into(), user.shell.clone()),
        ("PATH".into(), "/usr/local/bin:/usr/bin:/bin:/usr/local/sbin:/usr/sbin".into()),
        ("TERM".into(), env::var("TERM").unwrap_or_else(|_| "linux".into())),
        ("XDG_SESSION_CLASS".into(), "user".into()),
        ("XDG_SEAT".into(), "seat0".into()),
        ("XDG_VTNR".into(), tty.to_string()),
    ];

    let xdg_runtime_dir = format!("/run/user/{}", user.uid);
    envs.push(("XDG_RUNTIME_DIR".into(), xdg_runtime_dir));

    match session.session_type {
        SessionType::Wayland => {
            envs.push(("XDG_SESSION_TYPE".into(), "wayland".into()));
            if let Some(ref desktop_names) = session.desktop_names {
                envs.push(("XDG_CURRENT_DESKTOP".into(), desktop_names.clone()));
                envs.push(("XDG_SESSION_DESKTOP".into(), desktop_names.clone()));
            } else {
                envs.push(("XDG_CURRENT_DESKTOP".into(), session.name.clone()));
                envs.push(("XDG_SESSION_DESKTOP".into(), session.name.clone()));
            }
        }
        SessionType::X11 => {
            envs.push(("XDG_SESSION_TYPE".into(), "x11".into()));
            envs.push(("DISPLAY".into(), format!(":{}", tty)));
            if let Some(ref desktop_names) = session.desktop_names {
                envs.push(("XDG_CURRENT_DESKTOP".into(), desktop_names.clone()));
                envs.push(("XDG_SESSION_DESKTOP".into(), desktop_names.clone()));
            }
        }
        SessionType::Shell => {
            envs.push(("XDG_SESSION_TYPE".into(), "tty".into()));
        }
    }

    // Prepare command
    let (prog, args): (String, Vec<String>) = match session.session_type {
        SessionType::Wayland | SessionType::Shell => {
            ("/bin/sh".into(), vec!["-l".into(), "-c".into(), session.exec.clone()])
        }
        SessionType::X11 => {
            if std::path::Path::new("/usr/bin/xinit").exists() {
                (
                    "/usr/bin/xinit".into(),
                    vec![
                        session.exec.clone(),
                        "--".into(),
                        format!(":{}", tty),
                        vt_str,
                    ],
                )
            } else {
                ("/bin/sh".into(), vec!["-l".into(), "-c".into(), session.exec.clone()])
            }
        }
    };

    let uid = nix::unistd::Uid::from_raw(user.uid);
    let gid = nix::unistd::Gid::from_raw(user.gid);
    let home_dir = user.dir.clone();
    let username_c = CString::new(user.name.clone()).unwrap();

    let mut cmd = Command::new(prog);
    cmd.args(&args);

    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.current_dir(&home_dir);

    unsafe {
        cmd.pre_exec(move || {
            // Set process group / session leader
            if let Err(e) = nix::unistd::setsid() {
                eprintln!("setsid failed: {:?}", e);
            }

            // Init supplementary groups
            if let Err(e) = nix::unistd::initgroups(&username_c, gid) {
                eprintln!("initgroups failed: {:?}", e);
            }

            // Set gid and uid
            if let Err(e) = nix::unistd::setgid(gid) {
                eprintln!("setgid failed: {:?}", e);
                return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "setgid failed"));
            }

            if let Err(e) = nix::unistd::setuid(uid) {
                eprintln!("setuid failed: {:?}", e);
                return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "setuid failed"));
            }

            Ok(())
        });
    }

    // Execute session (replaces current process if successful via exec)
    let err = cmd.exec();
    Err(err)
}
