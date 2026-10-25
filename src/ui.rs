use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear as RatatuiClear, Paragraph},
    Frame, Terminal,
};
use std::io;

use crate::auth;
use crate::config::Config;
use crate::session::{SessionEntry, SessionType};
use crate::user::UserEntry;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FocusedField {
    User,
    Password,
    Session,
    Power,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PowerAction {
    Shutdown,
    Reboot,
    Cancel,
}

pub struct AppState {
    pub hostname: String,
    pub users: Vec<UserEntry>,
    pub sessions: Vec<SessionEntry>,
    pub config: Config,
    pub selected_user_idx: usize,
    pub user_input: String,
    pub password_input: String,
    pub selected_session_idx: usize,
    pub focused_field: FocusedField,
    pub error_message: Option<String>,
    pub show_power_dialog: bool,
    pub power_action: PowerAction,
}

impl AppState {
    pub fn new(users: Vec<UserEntry>, sessions: Vec<SessionEntry>, config: Config) -> Self {
        let hostname = nix::unistd::gethostname()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "localhost".to_string());

        let mut user_input = String::new();
        let mut selected_user_idx = 0;

        if let Some(ref def_u) = config.default_user {
            if let Some(idx) = users.iter().position(|u| &u.name == def_u) {
                selected_user_idx = idx;
                user_input = def_u.clone();
            } else {
                user_input = def_u.clone();
            }
        } else if let Some(first) = users.first() {
            user_input = first.name.clone();
        }

        let mut selected_session_idx = 0;
        if let Some(ref def_s) = config.default_session
            && let Some(idx) = sessions.iter().position(|s| &s.name == def_s)
        {
            selected_session_idx = idx;
        }

        Self {
            hostname,
            users,
            sessions,
            config,
            selected_user_idx,
            user_input,
            password_input: String::new(),
            selected_session_idx,
            focused_field: FocusedField::User,
            error_message: None,
            show_power_dialog: false,
            power_action: PowerAction::Shutdown,
        }
    }
}

pub fn run_ui(
    mut state: AppState,
) -> Result<Option<(UserEntry, SessionEntry)>, Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();

    // Clear entire terminal and screen buffer
    execute!(
        stdout,
        EnterAlternateScreen,
        Clear(ClearType::All),
        Clear(ClearType::Purge),
        cursor::MoveTo(0, 0)
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run_loop(&mut terminal, &mut state);

    // Completely clear terminal on exit so no UI artifacts stay on tty
    let mut backend_stdout = io::stdout();
    let _ = disable_raw_mode();
    let _ = execute!(
        backend_stdout,
        Clear(ClearType::All),
        Clear(ClearType::Purge),
        cursor::MoveTo(0, 0),
        LeaveAlternateScreen,
        cursor::Show
    );

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
) -> Result<Option<(UserEntry, SessionEntry)>, Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|f| draw_ui(f, state))?;

        if event::poll(std::time::Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if state.show_power_dialog {
                match key.code {
                    KeyCode::Left | KeyCode::Char('h') => {
                        state.power_action = match state.power_action {
                            PowerAction::Shutdown => PowerAction::Cancel,
                            PowerAction::Reboot => PowerAction::Shutdown,
                            PowerAction::Cancel => PowerAction::Reboot,
                        };
                    }
                    KeyCode::Right | KeyCode::Tab | KeyCode::Char('l') => {
                        state.power_action = match state.power_action {
                            PowerAction::Shutdown => PowerAction::Reboot,
                            PowerAction::Reboot => PowerAction::Cancel,
                            PowerAction::Cancel => PowerAction::Shutdown,
                        };
                    }
                    KeyCode::Enter => match state.power_action {
                        PowerAction::Shutdown => {
                            let _ = std::process::Command::new("poweroff").spawn();
                            return Ok(None);
                        }
                        PowerAction::Reboot => {
                            let _ = std::process::Command::new("reboot").spawn();
                            return Ok(None);
                        }
                        PowerAction::Cancel => {
                            state.show_power_dialog = false;
                        }
                    },
                    KeyCode::Esc => {
                        state.show_power_dialog = false;
                    }
                    _ => {}
                }
                continue;
            }

            // Shortcuts
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('c') => return Ok(None),
                    KeyCode::Char('p') => {
                        state.show_power_dialog = true;
                        continue;
                    }
                    _ => {}
                }
            }

            match key.code {
                KeyCode::F(12) => {
                    state.show_power_dialog = true;
                }
                KeyCode::Tab | KeyCode::Down => {
                    state.error_message = None;
                    state.focused_field = match state.focused_field {
                        FocusedField::User => FocusedField::Password,
                        FocusedField::Password => FocusedField::Session,
                        FocusedField::Session => FocusedField::Power,
                        FocusedField::Power => FocusedField::User,
                    };
                }
                KeyCode::BackTab | KeyCode::Up => {
                    state.error_message = None;
                    state.focused_field = match state.focused_field {
                        FocusedField::User => FocusedField::Power,
                        FocusedField::Password => FocusedField::User,
                        FocusedField::Session => FocusedField::Password,
                        FocusedField::Power => FocusedField::Session,
                    };
                }
                KeyCode::Left => match state.focused_field {
                    FocusedField::User => {
                        if !state.users.is_empty() {
                            if state.selected_user_idx == 0 {
                                state.selected_user_idx = state.users.len() - 1;
                            } else {
                                state.selected_user_idx -= 1;
                            }
                            state.user_input = state.users[state.selected_user_idx].name.clone();
                            state.password_input.clear();
                            state.error_message = None;
                        }
                    }
                    FocusedField::Session if !state.sessions.is_empty() => {
                        if state.selected_session_idx == 0 {
                            state.selected_session_idx = state.sessions.len() - 1;
                        } else {
                            state.selected_session_idx -= 1;
                        }
                    }
                    _ => {}
                },
                KeyCode::Right => match state.focused_field {
                    FocusedField::User => {
                        if !state.users.is_empty() {
                            state.selected_user_idx =
                                (state.selected_user_idx + 1) % state.users.len();
                            state.user_input = state.users[state.selected_user_idx].name.clone();
                            state.password_input.clear();
                            state.error_message = None;
                        }
                    }
                    FocusedField::Session if !state.sessions.is_empty() => {
                        state.selected_session_idx =
                            (state.selected_session_idx + 1) % state.sessions.len();
                    }
                    _ => {}
                },
                KeyCode::Char(c) => match state.focused_field {
                    FocusedField::User => {
                        state.error_message = None;
                        state.user_input.push(c);
                    }
                    FocusedField::Password => {
                        state.error_message = None;
                        state.password_input.push(c);
                    }
                    FocusedField::Power => {
                        if c == 's' || c == 'S' {
                            state.power_action = PowerAction::Shutdown;
                            state.show_power_dialog = true;
                        } else if c == 'r' || c == 'R' {
                            state.power_action = PowerAction::Reboot;
                            state.show_power_dialog = true;
                        }
                    }
                    _ => {}
                },
                KeyCode::Backspace => match state.focused_field {
                    FocusedField::User => {
                        state.user_input.pop();
                        state.error_message = None;
                    }
                    FocusedField::Password => {
                        state.password_input.pop();
                        state.error_message = None;
                    }
                    _ => {}
                },
                KeyCode::Enter => {
                    if state.focused_field == FocusedField::Power {
                        state.show_power_dialog = true;
                        continue;
                    }

                    if state.user_input.trim().is_empty() {
                        state.error_message = Some("empty user".to_string());
                        state.focused_field = FocusedField::User;
                        continue;
                    }

                    if state.password_input.is_empty() {
                        state.focused_field = FocusedField::Password;
                        continue;
                    }

                    // Authenticate
                    let username = state.user_input.trim();
                    match auth::authenticate(
                        &state.config.pam_service,
                        username,
                        &state.password_input,
                    ) {
                        Ok(()) => {
                            let user_entry = state
                                .users
                                .iter()
                                .find(|u| u.name == username)
                                .cloned()
                                .unwrap_or_else(|| {
                                    let u = users::get_user_by_name(username);
                                    UserEntry {
                                        name: username.to_string(),
                                        uid: u.as_ref().map(|x| x.uid()).unwrap_or(1000),
                                        gid: u
                                            .as_ref()
                                            .map(|x| x.primary_group_id())
                                            .unwrap_or(1000),
                                        dir: format!("/home/{}", username),
                                        shell: "/bin/sh".to_string(),
                                    }
                                });

                            let session_entry = if !state.sessions.is_empty() {
                                state.sessions[state.selected_session_idx].clone()
                            } else {
                                SessionEntry {
                                    name: "sh".into(),
                                    exec: "/bin/sh -l".into(),
                                    session_type: SessionType::Shell,
                                    desktop_names: None,
                                }
                            };

                            return Ok(Some((user_entry, session_entry)));
                        }
                        Err(_) => {
                            state.error_message = Some("login incorrect".to_string());
                            if state.config.clear_password_on_error {
                                state.password_input.clear();
                            }
                            if state.config.clear_username_on_error {
                                state.user_input.clear();
                                state.focused_field = FocusedField::User;
                            } else {
                                state.focused_field = FocusedField::Password;
                            }
                        }
                    }
                }
                KeyCode::Esc => {
                    state.password_input.clear();
                    state.error_message = None;
                }
                _ => {}
            }
        }
    }
}

fn draw_ui(f: &mut Frame, state: &AppState) {
    let size = f.area();

    // 1. Clean backdrop (empty black/default terminal, no noisy outer borders)
    f.render_widget(RatatuiClear, size);

    // 2. Compact clean login box in center
    let box_width = 46.min(size.width.saturating_sub(2));
    let box_height = 11.min(size.height.saturating_sub(2));

    let box_area = Rect {
        x: (size.width.saturating_sub(box_width)) / 2,
        y: (size.height.saturating_sub(box_height)) / 2,
        width: box_width,
        height: box_height,
    };

    let title = state
        .config
        .banner
        .clone()
        .unwrap_or_else(|| state.hostname.clone());

    let login_box = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(format!(" {} ", title))
        .title_alignment(Alignment::Center);

    f.render_widget(login_box, box_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // empty spacer
            Constraint::Length(1), // user
            Constraint::Length(1), // pass
            Constraint::Length(1), // session
            Constraint::Length(1), // empty spacer
            Constraint::Length(1), // error or hint
            Constraint::Length(1), // bottom power info
        ])
        .split(box_area);

    // User line
    let u_focus = state.focused_field == FocusedField::User;
    let u_prefix = if u_focus { " > login:    " } else { "   login:    " };
    let u_style = if u_focus {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let user_line = Line::from(vec![
        Span::styled(u_prefix, u_style),
        Span::styled(&state.user_input, Style::default().fg(Color::White)),
        if u_focus {
            Span::styled("_", Style::default().fg(Color::White).add_modifier(Modifier::SLOW_BLINK))
        } else {
            Span::raw("")
        },
    ]);
    f.render_widget(Paragraph::new(user_line), chunks[1]);

    // Password line
    let p_focus = state.focused_field == FocusedField::Password;
    let p_prefix = if p_focus { " > password: " } else { "   password: " };
    let p_style = if p_focus {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let asterisks = "*".repeat(state.password_input.len());
    let pass_line = Line::from(vec![
        Span::styled(p_prefix, p_style),
        Span::styled(asterisks, Style::default().fg(Color::White)),
        if p_focus {
            Span::styled("_", Style::default().fg(Color::White).add_modifier(Modifier::SLOW_BLINK))
        } else {
            Span::raw("")
        },
    ]);
    f.render_widget(Paragraph::new(pass_line), chunks[2]);

    // Session line
    let s_focus = state.focused_field == FocusedField::Session;
    let s_prefix = if s_focus { " > session:  " } else { "   session:  " };
    let s_style = if s_focus {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    let sess_name = if !state.sessions.is_empty() {
        let s = &state.sessions[state.selected_session_idx];
        let tag = match s.session_type {
            SessionType::Wayland => "wayland",
            SessionType::X11 => "x11",
            SessionType::Shell => "sh",
        };
        format!("< {} > ({})", s.name, tag)
    } else {
        "< shell >".to_string()
    };

    let sess_line = Line::from(vec![
        Span::styled(s_prefix, s_style),
        Span::styled(sess_name, Style::default().fg(Color::Cyan)),
    ]);
    f.render_widget(Paragraph::new(sess_line), chunks[3]);

    // Error or status
    if let Some(ref err) = state.error_message {
        let err_line = Line::from(vec![Span::styled(
            format!(" {}", err),
            Style::default().fg(Color::Red),
        )]);
        f.render_widget(Paragraph::new(err_line).alignment(Alignment::Center), chunks[5]);
    }

    // Power shortcut hint
    let pwr_focus = state.focused_field == FocusedField::Power;
    let pwr_style = if pwr_focus {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let pwr_text = Span::styled("[f12 power]", pwr_style);
    f.render_widget(
        Paragraph::new(Line::from(vec![pwr_text])).alignment(Alignment::Right),
        chunks[6],
    );

    // Power dialog modal
    if state.show_power_dialog {
        let d_width = 34.min(size.width.saturating_sub(4));
        let d_height = 5.min(size.height.saturating_sub(4));
        let d_area = Rect {
            x: (size.width.saturating_sub(d_width)) / 2,
            y: (size.height.saturating_sub(d_height)) / 2,
            width: d_width,
            height: d_height,
        };

        f.render_widget(RatatuiClear, d_area);

        let d_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(Color::Gray))
            .title(" power ");
        f.render_widget(d_block, d_area);

        let d_chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(d_area);

        let s_btn = if state.power_action == PowerAction::Shutdown {
            Span::styled("[poweroff]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(" poweroff ", Style::default().fg(Color::DarkGray))
        };
        let r_btn = if state.power_action == PowerAction::Reboot {
            Span::styled("[reboot]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(" reboot ", Style::default().fg(Color::DarkGray))
        };
        let c_btn = if state.power_action == PowerAction::Cancel {
            Span::styled("[cancel]", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(" cancel ", Style::default().fg(Color::DarkGray))
        };

        let row = Line::from(vec![s_btn, Span::raw(" "), r_btn, Span::raw(" "), c_btn]);
        f.render_widget(Paragraph::new(row).alignment(Alignment::Center), d_chunks[0]);
    }
}
