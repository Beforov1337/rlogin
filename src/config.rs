use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub pam_service: String,
    pub default_user: Option<String>,
    pub default_session: Option<String>,
    pub banner: Option<String>,
    pub clear_password_on_error: bool,
    pub clear_username_on_error: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pam_service: "login".to_string(),
            default_user: None,
            default_session: None,
            banner: None,
            clear_password_on_error: true,
            clear_username_on_error: false,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let paths = [
            "/etc/rlogin/config.toml",
            "/etc/rlogin.toml",
            "config.toml",
        ];

        for path in &paths {
            if Path::new(path).exists()
                && let Ok(content) = fs::read_to_string(path)
                && let Ok(cfg) = toml::from_str::<Config>(&content)
            {
                return cfg;
            }
        }

        Config::default()
    }
}
