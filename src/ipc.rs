use std::convert::TryFrom;
use std::io;
use std::io::{Error, ErrorKind};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

use log::{error, info, warn};
use nix::unistd::{Gid, Uid};

pub const INBOX_SOCKET_PATH: &str = "/tmp/lemurs.inbox";
pub const OUTBOX_SOCKET_PATH: &str = "/tmp/lemurs.outbox";

pub struct IncomingSocket {
    path: String,
    listener: UnixListener,
    do_log: bool,
}

pub struct OutgoingSocket {
    stream: UnixStream,
}

impl IncomingSocket {
    pub fn new(path: impl Into<String>, do_log: bool, uid: u32, gid: u32) -> io::Result<Self> {
        let path = path.into();
        let listener = UnixListener::bind(path.clone())?;

        // Change permissions of listener
        nix::unistd::chown(
            path.as_bytes(),
            Some(Uid::from_raw(uid)),
            Some(Gid::from_raw(gid)),
        )
        .map_err(|err| io::Error::new(ErrorKind::Other, format!("Chown error: {}", err)))?;

        Ok(Self {
            path,
            listener,
            do_log,
        })
    }

    pub fn block_handle<F>(&self, handler: F) -> io::Result<()>
    where
        F: Fn(IpcRequest) -> io::Result<bool>,
    {
        for socket in self.listener.incoming() {
            if self.do_log {
                info!("Got an incoming connection to the socket");
            }

            match socket {
                Ok(mut socket) => {
                    let mut buf = [0u8; 1];
                    match socket.read(&mut buf) {
                        Ok(1) => match buf[0].try_into() {
                            Ok(req) => {
                                if self.do_log {
                                    info!("Received a logout request from the user");
                                }
                                match handler(req) {
                                    Ok(true) => return Ok(()),
                                    Ok(false) => {}
                                    Err(err) => {
                                        if self.do_log {
                                            warn!(
                                                "Failed to handle incoming message. Reason: {}",
                                                err
                                            );
                                        }
                                    }
                                }
                            }
                            _ => {
                                if self.do_log {
                                    warn!("Unexpected data received. Byte: '{}'", buf[0]);
                                }
                            }
                        },
                        Ok(_) => {
                            if self.do_log {
                                warn!(
                                    "Invalid amount of bytes received to interpret socket message"
                                );
                            }
                        }
                        Err(err) => {
                            if self.do_log {
                                warn!("Failed to read socket data. Reason: {}", err);
                            }
                        }
                    }
                }
                Err(err) => {
                    if self.do_log {
                        warn!(
                            "Got a request to connect to '{}' but failed to accept request. Reason: {}",
                            self.path, err
                        );
                    }
                }
            }
        }

        Ok(())
    }
}

const LOGOUT_WAIT_TIMEOUT: Duration = Duration::from_secs(1);

#[repr(u8)]
pub enum IpcRequest {
    Logout,
    Ack,
}

impl From<IpcRequest> for u8 {
    fn from(msg: IpcRequest) -> Self {
        use IpcRequest::*;

        match msg {
            Logout => 0,
            Ack => 1,
        }
    }
}

impl TryFrom<u8> for IpcRequest {
    type Error = ();

    fn try_from(num: u8) -> Result<Self, Self::Error> {
        use IpcRequest::*;

        match num {
            0 => Ok(Logout),
            1 => Ok(Ack),
            _ => Err(()),
        }
    }
}

pub fn send_for_logout() -> std::io::Result<()> {
    let listener = UnixListener::bind(OUTBOX_SOCKET_PATH)?;

    message_to_inbox(IpcRequest::Logout)?;

    let socket = listener.incoming().next().unwrap();
    let mut socket = socket?;
    socket.set_read_timeout(Some(LOGOUT_WAIT_TIMEOUT))?;
    let mut buf = [0u8; 1];
    match socket.read(&mut buf)? {
        1 => match buf[0].try_into() {
            Ok(IpcRequest::Ack) => Ok(()),
            _ => Err(Error::new(
                ErrorKind::Other,
                format!(
                    "Expected a logout message but got a different message. ID: {}",
                    buf[0]
                ),
            )),
        },
        _ => Err(Error::new(
            ErrorKind::Other,
            format!("Invalid amount of bytes received to interpret socket message"),
        )),
    }
}

pub fn message_to_inbox(msg: IpcRequest) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(INBOX_SOCKET_PATH)?;
    let buf = [msg.into()];
    stream.flush()?;
    stream.write(&buf)?;

    Ok(())
}

pub fn message_to_outbox(msg: IpcRequest) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(OUTBOX_SOCKET_PATH)?;
    let buf = [msg.into()];
    stream.flush()?;
    stream.write(&buf)?;

    Ok(())
}
