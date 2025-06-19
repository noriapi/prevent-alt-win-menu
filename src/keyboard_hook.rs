use std::{
    sync::{OnceLock, mpsc},
    thread,
};

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

static GLOBAL_SENDER: OnceLock<mpsc::Sender<KeyboardEvent>> = const { OnceLock::new() };

pub fn start_keyboard_hook() -> Result<(mpsc::Receiver<KeyboardEvent>, thread::JoinHandle<()>)> {
    let (tx, rx) = mpsc::channel::<KeyboardEvent>();
    GLOBAL_SENDER
        .set(tx)
        .map_err(|_| Error::AlreadyInitialized)?;

    Ok((
        rx,
        thread::spawn(|| {
            let _handle = unsafe { register_keyboard_hook(Some(low_level_keyboard_proc)) }
                .inspect_err(|_e| {
                    #[cfg(feature = "log")]
                    log::error!("Failed to register keyboard hook: {}", _e);
                })
                .expect("Failed to register keyboard hook");

            #[cfg(feature = "log")]
            log::info!("registered keybord hook");

            let mut msg = MSG::default();
            unsafe {
                while GetMessageW(&mut msg, None, 0, 0).into() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            };
        }),
    ))
}

unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code == HC_ACTION as i32 {
        let event = unsafe { KeyboardEvent::from_params(l_param, w_param) };

        if let Some(sender) = GLOBAL_SENDER.get() {
            if let Err(_e) = sender.send(event) {
                #[cfg(feature = "log")]
                log::error!("{}", _e);
            }
        };
    }

    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}

unsafe fn register_keyboard_hook(f: HOOKPROC) -> Result<Owned<HHOOK>> {
    let keyboard_hook = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            f,
            Some(GetModuleHandleW(None).unwrap().into()),
            0,
        )
    }?;

    if keyboard_hook.is_invalid() {
        return Err(std::io::Error::last_os_error().into());
    }

    Ok(unsafe { Owned::new(keyboard_hook) })
}
