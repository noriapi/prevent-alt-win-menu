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
