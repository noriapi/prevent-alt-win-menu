#![doc = include_str!("../README.md")]

pub mod error;
pub mod event_handler;
pub mod keyboard_hook;

use std::thread;

use error::Result;
use event_handler::Config;

/// Starts keyboard hook and event handler threads to suppress the Alt or Windows menu.
///
/// This function installs a low-level keyboard hook that listens for key events
/// and spawns a thread to handle suppression logic. It returns two [`std::thread::JoinHandle`]s:
/// one for the keyboard hook thread and one for the event handler thread.
///
/// You may choose to ignore the returned [`JoinHandles`] entirely.
/// The suppression behavior will remain active as long as both threads are running.
///
/// # Errors
///
/// Returns an error if the keyboard hook cannot be registered or the hook thread fails to initialize.
pub fn start(config: Config) -> Result<JoinHandles> {
    let (rx, hook_handle) = keyboard_hook::start_keyboard_hook()?;
    let handler_handle = event_handler::start_event_handler(rx, config);

    Ok(JoinHandles {
        keyboard_hook: hook_handle,
        event_handler: handler_handle,
    })
}

/// Pair of thread handles for the keyboard hook and event handler.
///
/// These are standard [`std::thread::JoinHandle`]s representing background threads
/// that suppress the system menu triggered by Alt or Windows key releases.
///
/// In typical usage, you do not need to hold on to this struct:
/// the threads will continue running in the background as long as the application does.
///
/// However, if you want to explicitly wait for their termination or check for errors,
/// you can keep and `join()` them as needed.
pub struct JoinHandles {
    /// Thread that runs the Windows low-level keyboard hook.
    pub keyboard_hook: thread::JoinHandle<()>,

    /// Thread that processes keyboard events and performs suppression.
    pub event_handler: thread::JoinHandle<()>,
}
