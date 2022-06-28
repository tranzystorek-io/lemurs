use log::{error, info, warn};
use nix::unistd::{getgid, getgroups, getuid};
use std::fs;

use crate::auth::AuthUserInfo;
use crate::config::Config;
use crate::ipc::{
    message_to_outbox, send_for_logout, IncomingSocket, IpcRequest, INBOX_SOCKET_PATH,
};
use crate::tty::switch_to_lemurs_tty;
use env_variables::{init_environment, set_xdg_env};

mod env_variables;
mod x;

const INITRCS_FOLDER_PATH: &str = "/etc/lemurs/wms";

#[derive(Clone)]
pub enum PostLoginEnvironment {
    X { xinitrc_path: String },
    Wayland { script_path: String },
    Shell,
}

pub enum EnvironmentStartError {
    OpenInbox(std::io::Error),
    HandleLogout(std::io::Error),
    XSetupError(x::XSetupError),
    XStartEnvError(x::XStartEnvError),
}

impl PostLoginEnvironment {
    pub fn start<'a>(
        &self,
        config: &Config,
        user_info: AuthUserInfo<'a>,
    ) -> Result<(), EnvironmentStartError> {
        init_environment(&user_info.name, &user_info.dir, &user_info.shell);
        info!("Set environment variables.");

        set_xdg_env(user_info.uid, &user_info.dir, config.tty);
        info!("Set XDG environment variables");

        match self {
            PostLoginEnvironment::X { xinitrc_path } => {
                let inbox_listener =
                    IncomingSocket::new(INBOX_SOCKET_PATH, true, user_info.uid, user_info.gid)
                        .map_err(|err| {
                            error!("Failed to bind to inbox socket. Reason: {}", err);
                            EnvironmentStartError::OpenInbox(err)
                        })?;

                let mut x_server =
                    x::setup_x(&user_info).map_err(EnvironmentStartError::XSetupError)?;
                info!("Started X server");

                let mut gui_environment = x::start_env(&user_info, xinitrc_path)
                    .map_err(EnvironmentStartError::XStartEnvError)?;
                info!("Started GUI environment");

                info!("Lemurs started waiting for logout signal");
                inbox_listener
                    .block_handle(|request| {
                        Ok(match request {
                            IpcRequest::Logout => {
                                message_to_outbox(IpcRequest::Ack)?;
                                true
                            }
                            _ => false,
                        })
                    })
                    .map_err(|err| {
                        error!("Failed to bind to inbox socket. Reason: {}", err);
                        EnvironmentStartError::HandleLogout(err)
                    })?;

                switch_to_lemurs_tty(config.tty);

                info!("Killing GUI environment");
                if let Err(err) = gui_environment.kill() {
                    warn!("Failed to kill gui_environment. Reason: {}", err);
                }

                info!("Killing X server");
                if let Err(err) = x_server.kill() {
                    warn!("Failed to kill X server. Reason: {}", err);
                }

                // Effectively removing any authentication
                drop(user_info);
            }
            _ => unimplemented!(),
        }

        Ok(())
    }
}

pub fn get_envs() -> Vec<(String, PostLoginEnvironment)> {
    let found_paths = match fs::read_dir(INITRCS_FOLDER_PATH) {
        Ok(paths) => paths,
        Err(_) => return Vec::new(),
    };

    // NOTE: Maybe we can do something smart with `with_capacity` here.
    let mut envs = Vec::new();

    // TODO: Add other post login environment methods
    for path in found_paths {
        if let Ok(path) = path {
            let file_name = path.file_name().into_string();

            if let Ok(file_name) = file_name {
                envs.push((
                    file_name,
                    PostLoginEnvironment::X {
                        // TODO: Remove unwrap
                        xinitrc_path: path.path().to_str().unwrap().to_string(),
                    },
                ));
            } else {
                warn!("Unable to convert OSString to String");
            }
        } else {
            warn!("Ignored errorinous path: '{}'", path.unwrap_err());
        }
    }

    envs
}
