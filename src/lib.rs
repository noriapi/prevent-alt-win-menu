//! Prevents the menu bar or Start menu from appearing when the Alt or Windows key is
//! released on Windows.

pub mod error;
pub mod event_handler;
pub mod keyboard_hook;

use std::thread;

use error::Result;
use event_handler::Config;

pub fn start(config: Config) -> Result<JoinHandles> {
    let (rx, hook_handle) = keyboard_hook::start_keyboard_hook()?;
    let handler_handle = event_handler::start_event_handler(rx, config);

    Ok(JoinHandles {
        keyboard_hook: hook_handle,
        event_handler: handler_handle,
    })
}

pub struct JoinHandles {
    pub keyboard_hook: thread::JoinHandle<()>,
    pub event_handler: thread::JoinHandle<()>,
}

impl JoinHandles {
    pub fn join(self) -> thread::Result<()> {
        self.keyboard_hook.join()?;
        self.event_handler.join()?;
        Ok(())
    }
}
