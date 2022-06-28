#![feature(setgroups)]

use std::error::Error;
use std::io;
use std::io::ErrorKind;
use std::process;
use std::time::Duration;

use clap::{arg, App as ClapApp};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::{error, info, warn};
use tui::backend::CrosstermBackend;
use tui::Terminal;

mod auth;
mod config;
mod info_caching;
mod ipc;
mod post_login;
mod tty;
mod ui;

use auth::{try_auth, AuthUserInfo};
use config::Config;
use post_login::{EnvironmentStartError, PostLoginEnvironment};

use crate::tty::switch_to_lemurs_tty;

const DEFAULT_CONFIG_PATH: &str = "/etc/lemurs/config.toml";
const PREVIEW_LOG_PATH: &str = "lemurs.log";
const DEFAULT_LOG_PATH: &str = "/var/log/lemurs.log";

fn is_in_session() -> bool {
    return std::env::vars().any(|(key, value)| key == "USER" && !value.is_empty());
}

fn merge_in_configuration(config: &mut Config, config_path: Option<&str>) {
    let load_config_path = config_path.unwrap_or(DEFAULT_CONFIG_PATH);

    match config::PartialConfig::from_file(load_config_path) {
        Ok(partial_config) => {
            info!(
                "Successfully loaded configuration file from '{}'",
                load_config_path
            );
            config.merge_in_partial(partial_config)
        }
        Err(err) => {
            // If we have given it a specific config path, it should crash if this file cannot be
            // loaded. If it is the default config location just put a warning in the logs.
            if let Some(config_path) = config_path {
                eprintln!(
                    "The config file '{}' cannot be loaded.\nReason: {}",
                    config_path, err
                );
                process::exit(1);
            } else {
                warn!(
                    "No configuration file loaded from the expected location ({}). Reason: {}",
                    DEFAULT_CONFIG_PATH, err
                );
            }
        }
    }
}

fn setup_logger(is_preview: bool) {
    let log_path = if is_preview {
        PREVIEW_LOG_PATH
    } else {
        DEFAULT_LOG_PATH
    };

    let log_file = fern::log_file(log_path).unwrap_or_else(|err| {
        eprintln!(
            "Failed to open log file: '{}'. Check that the path is valid or activate `--no-log`. Reason: {}",
            log_path, err
        );
        process::exit(1);
    });

    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .level_for("hyper", log::LevelFilter::Info)
        .chain(log_file)
        .apply()
        .unwrap_or_else(|err| {
            eprintln!(
                "Failed to setup logger. Fix the error or activate `--no-log`. Reason: {}",
                err
            );
            process::exit(1);
        });
}

fn main() -> Result<(), Box<dyn Error>> {
    let matches = ClapApp::new("Lemurs")
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(arg!(--preview))
        .arg(arg!(--nolog))
        .arg(arg!(-q --logout "Logout from the current lemurs session"))
        .arg(arg!(-c --config [FILE] "a file to replace the default configuration"))
        .get_matches();

    let no_log = matches.is_present("nolog");
    let preview = matches.is_present("preview");
    let logout = matches.is_present("logout");
    let is_in_session = is_in_session();

    if is_in_session && !(preview || logout) {
        eprintln!("Already in an authenticated session. You can either run with `--preview` or `--logout`.");
        std::process::exit(1);
    }

    if logout {
        // Prevent logging out when not in a session
        if !is_in_session {
            eprintln!("Cannot logout without being in an authenticated session");
            std::process::exit(1);
        }

        println!("Requesting a logout");
        match ipc::send_for_logout() {
            Ok(_) => {
                println!("Logging out...");
                std::thread::sleep(Duration::from_secs(1));
                process::exit(0);
            }
            Err(err) => {
                match err.kind() {
                    ErrorKind::PermissionDenied => {
                        eprintln!(
                            "No permission to logout. Logout should be run with root priveledges"
                        )
                    }
                    _ => {
                        eprintln!("Failed to communicate with Lemurs session. Reason: {}", err)
                    }
                };
                process::exit(1);
            }
        }
    }

    // Setup the logger
    if !no_log {
        setup_logger(preview);
    }

    info!("Lemurs logger is running");

    // Load and setup configuration
    let mut config = Config::default();
    merge_in_configuration(&mut config, matches.value_of("config"));

    if !preview {
        switch_to_lemurs_tty(config.tty);
    }

    // Start application
    let mut terminal = tui_enable()?;
    let login_form = ui::LoginForm::new(config, preview);
    login_form.run(&mut terminal, try_auth, post_login_env_start)?;
    tui_disable(terminal)?;

    println!("Lemurs terminated...");
    info!("Lemurs is booting down");

    Ok(())
}

pub fn tui_enable() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    info!("UI booted up");

    Ok(terminal)
}

pub fn tui_disable(mut terminal: Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    info!("Reset terminal environment");

    Ok(())
}

fn post_login_env_start<'a>(
    post_login_env: &PostLoginEnvironment,
    config: &Config,
    user_info: AuthUserInfo<'a>,
) -> Result<(), EnvironmentStartError> {
    post_login_env.start(config, user_info)
}
