use log::{error, info};

pub fn switch_to_lemurs_tty(tty: u8) {
    // Switch to the proper tty
    info!("Switching to tty {}", tty);

    chvt::chvt(tty.into()).unwrap_or_else(|err| {
        error!("Failed to switch tty {}. Reason: {}", tty, err);
    });
}
