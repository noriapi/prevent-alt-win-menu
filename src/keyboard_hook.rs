//! Low-level module for starting a global keyboard hook on Windows.
//!
//! This module registers a system-wide low-level keyboard hook (`WH_KEYBOARD_LL`)
//! and sends captured events as [`KeyboardEvent`]s through a channel.
//!
//! In most cases, it is recommended to use the higher-level API [`crate::start`].
//! Use this module directly only if you need custom keyboard event handling
//! or fine-grained control over the hook behavior.
//!
//! # Public API
//! - [`start_keyboard_hook`] — Starts the global keyboard hook and returns a receiver and thread handle.
use std::{cell::OnceCell, sync::mpsc, thread};

use windows::{
    Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, HHOOK, HOOKPROC, MSG,
            SetWindowsHookExW, TranslateMessage, WH_KEYBOARD_LL,
        },
    },
    core::Owned,
};

use crate::{
    error::{Error, Result},
    event_handler::KeyboardEvent,
};

thread_local! {
    static GLOBAL_SENDER: OnceCell<mpsc::Sender<KeyboardEvent>> = const { OnceCell::new() };
}

/// Starts a global keyboard hook and spawns a thread to handle incoming events.
///
/// This function registers a low-level Windows keyboard hook that captures all
/// keyboard input events system-wide and sends them through a channel.
///
/// The hook is run on a background thread. The function returns a `Receiver`
/// for incoming `KeyboardEvent`s and the `JoinHandle` for the background thread.
///
/// # Returns
/// - `Ok((rx, handle))`:
///   - `rx`: A receiver that delivers captured keyboard events.
///   - `handle`: A join handle for the background thread running the hook loop.
///
/// # Errors
/// - Returns `Error::HookRegistrationFailed` if the keyboard hook fails to register.
/// - Returns `Error::HookThreadCrashed` if the hook thread terminated unexpectedly.
///
/// # Note
/// - Unhooking is not currently implemented. The hook will be released automatically when the process exits.
pub fn start_keyboard_hook() -> Result<(mpsc::Receiver<KeyboardEvent>, thread::JoinHandle<()>)> {
    let (tx, rx) = mpsc::channel::<KeyboardEvent>();

    let (result_tx, result_rx) = oneshot::channel::<Result<()>>();

    let join_handle = thread::spawn(move || {
        GLOBAL_SENDER.with(|g| g.set(tx)).unwrap();

        let hook_result = unsafe { register_keyboard_hook(Some(low_level_keyboard_proc)) };

        let _hook_handle = match hook_result {
            Err(e) => {
                #[cfg(feature = "log")]
                log::error!("Failed to register keyboard hook: {}", e);
                let _ = result_tx.send(Err(Error::HookRegistrationFailed(e)));
                return;
            }
            Ok(handle) => {
                let _ = result_tx.send(Ok(()));
                handle
            }
        };

        #[cfg(feature = "log")]
        log::info!("registered keybord hook");

        let mut msg = MSG::default();
        unsafe {
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });

    match result_rx.recv() {
        Ok(Ok(_)) => Ok((rx, join_handle)),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(Error::HookThreadCrashed),
    }
}

unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code == HC_ACTION as i32 {
        let event = unsafe { KeyboardEvent::from_params(l_param, w_param) };

        GLOBAL_SENDER.with(|s| {
            let sender = s.get().unwrap();
            if let Err(_e) = sender.send(event) {
                #[cfg(feature = "log")]
                log::error!("{}", _e);
            }
        })
    }

    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}

unsafe fn register_keyboard_hook(f: HOOKPROC) -> std::io::Result<Owned<HHOOK>> {
    let keyboard_hook = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            f,
            Some(GetModuleHandleW(None).unwrap().into()),
            0,
        )
    }?;

    Ok(unsafe { Owned::new(keyboard_hook) })
}
