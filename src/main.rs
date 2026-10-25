mod auth;
mod config;
mod session;
mod ui;
mod user;

use std::env;

fn main() {
    let mut args = env::args().skip(1);
    let mut tty_num = None;

    while let Some(arg) = args.next() {
        if (arg == "--tty" || arg == "-t")
            && let Some(t) = args.next() {
                tty_num = t.parse::<u32>().ok();
            }
    }

    // If not passed explicitly, attempt to parse from current tty (e.g. /dev/tty2)
    if tty_num.is_none()
        && let Ok(tty_name) = nix::unistd::ttyname(std::io::stdin())
            && let Some(lossy) = tty_name.to_str()
                && let Some(num_str) = lossy.strip_prefix("/dev/tty") {
                    tty_num = num_str.parse::<u32>().ok();
                }

    // 1. Load config
    let cfg = config::Config::load();

    // 2. Auto-detect users and sessions
    let users = user::detect_users();
    let sessions = session::detect_sessions();

    // 3. Initialize state and run TUI loop
    let state = ui::AppState::new(users, sessions, cfg);

    match ui::run_ui(state) {
        Ok(Some((user_entry, session_entry))) => {
            // Authentication succeeded, launch session
            if let Err(e) = auth::spawn_session(&user_entry, &session_entry, tty_num) {
                eprintln!("Failed to launch session: {}", e);
                std::process::exit(1);
            }
        }
        Ok(None) => {
            // Cancelled or exited
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("TUI Error: {}", e);
            std::process::exit(1);
        }
    }
}
