use std::{
    sync::{
        OnceLock,
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use thiserror::Error;
pub use windows::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT;
use windows::{
    Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{
                INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
                VIRTUAL_KEY, VK__none_, VK_LMENU, VK_LWIN, VK_MENU, VK_RMENU, VK_RWIN,
            },
            WindowsAndMessaging::{
                CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, MSG, SetWindowsHookExW,
                TranslateMessage, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
            },
        },
    },
    core::Owned,
};

static GLOBAL_SENDER: OnceLock<mpsc::Sender<KeyboardEvent>> = const { OnceLock::new() };

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyboardEvent {
    pub kbd: KBDLLHOOKSTRUCT,
    pub key_state: KeyState,
}

impl KeyboardEvent {
    unsafe fn from_params(l_param: LPARAM, w_param: WPARAM) -> KeyboardEvent {
        let kbd = unsafe { *(l_param.0 as *const KBDLLHOOKSTRUCT) };
        let key_state = KeyState::from_w_param(w_param).unwrap();
        Self { kbd, key_state }
    }

    pub fn virtual_key(&self) -> VIRTUAL_KEY {
        VIRTUAL_KEY(self.kbd.vkCode as _)
    }

    pub fn is_key_down(&self) -> bool {
        self.key_state.is_key_down()
    }

    pub fn is_key_up(&self) -> bool {
        self.key_state.is_key_up()
    }

    pub fn duration_since(&self, earlier: &KeyboardEvent) -> Duration {
        let millis = self.kbd.time.wrapping_sub(earlier.kbd.time);
        Duration::from_millis(millis as u64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    /// [WM_KEYDOWN](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-keydown)
    KeyDown,
    /// [WM_KEYUP](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-keyup)
    KeyUp,
    /// [WM_SYSKEYDOWN](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-syskeydown)
    SysKeyDown,
    /// [WM_SYSKEYUP](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-syskeyup)
    SysKeyUp,
}

impl KeyState {
    fn from_w_param(w_param: WPARAM) -> Option<KeyState> {
        if w_param.0 == WM_KEYDOWN as usize {
            Some(KeyState::KeyDown)
        } else if w_param.0 == WM_KEYUP as usize {
            Some(KeyState::KeyUp)
        } else if w_param.0 == WM_SYSKEYDOWN as usize {
            Some(KeyState::SysKeyDown)
        } else if w_param.0 == WM_SYSKEYUP as usize {
            Some(KeyState::SysKeyUp)
        } else {
            None
        }
    }

    pub fn is_key_down(&self) -> bool {
        matches!(self, KeyState::KeyDown | KeyState::SysKeyDown)
    }

    pub fn is_key_up(&self) -> bool {
        !self.is_key_down()
    }
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

#[derive(Default)]
pub struct PreventMenuOption {
    pub win: PreventMenuOptionItem,
    pub alt: PreventMenuOptionItem,
}

pub struct PreventMenuOptionItem {
    /// Key to prevent menu display
    ///
    /// Prevents the menu from appearing by misidentifying it as a simultaneous press of the Alt or Win key.
    pub dummy_key: VIRTUAL_KEY,

    /// Determine if menu display should be prevented
    ///
    /// The default is always to suppress the menu display.
    pub should_prevent: Box<ShouldPreventPredicate>,
}

impl PreventMenuOptionItem {
    pub fn from_threshold(duration: Duration) -> Self {
        Self::default().set_should_prevent(move |state| state.duration() > duration)
    }

    pub fn set_dummy_key(mut self, dummy_key: VIRTUAL_KEY) -> Self {
        self.dummy_key = dummy_key;
        self
    }

    pub fn set_should_prevent<F: Fn(ReleasedState) -> bool + Send + Sync + 'static>(
        mut self,
        should_prevent: F,
    ) -> Self {
        self.should_prevent = Box::new(should_prevent);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReleasedState {
    pub last_pressed_event: KeyboardEvent,
    pub released_event: KeyboardEvent,
}

impl ReleasedState {
    pub fn duration(&self) -> Duration {
        self.released_event.duration_since(&self.last_pressed_event)
    }
}

pub type ShouldPreventPredicate = dyn Fn(ReleasedState) -> bool + Send + Sync + 'static;

impl Default for PreventMenuOptionItem {
    fn default() -> Self {
        PreventMenuOptionItem {
            dummy_key: VK__none_,
            should_prevent: Box::new(always_true),
        }
    }
}

fn always_true(_: ReleasedState) -> bool {
    true
}

#[derive(Debug, Clone, Copy, Error)]
pub enum Error {
    #[error("already initialized")]
    AlreadyInitialized,
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn start(option: PreventMenuOption) -> Result<()> {
    let rx = start_listen()?;
    spawn_handler(rx, option);

    #[cfg(feature = "log")]
    log::info!("started");

    Ok(())
}

pub fn start_listen() -> Result<Receiver<KeyboardEvent>> {
    let rx = init_channel()?;

    spawn_listener();

    Ok(rx)
}

fn init_channel() -> Result<Receiver<KeyboardEvent>> {
    if GLOBAL_SENDER.get().is_some() {
        return Err(Error::AlreadyInitialized);
    }

    let (tx, rx) = mpsc::channel::<KeyboardEvent>();
    GLOBAL_SENDER.set(tx).unwrap();

    Ok(rx)
}

fn spawn_listener() -> JoinHandle<std::io::Result<()>> {
    debug_assert!(GLOBAL_SENDER.get().is_some());

    thread::spawn(|| {
        let keyboard_hook = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(low_level_keyboard_proc),
                Some(GetModuleHandleW(None).unwrap().into()),
                0,
            )
        }?;

        if keyboard_hook.is_invalid() {
            return Err(std::io::Error::last_os_error());
        }

        let _handle = unsafe { Owned::new(keyboard_hook) };

        let mut msg = MSG::default();
        unsafe {
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        Ok(())
    })
}

pub fn spawn_handler(rx: Receiver<KeyboardEvent>, option: PreventMenuOption) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut win_last_pressed = None;
        let mut alt_last_pressed = None;

        loop {
            let event = rx.recv().unwrap();

            match event.virtual_key() {
                VK_LWIN | VK_RWIN => {
                    handle_event("WIN", &event, &mut win_last_pressed, &option.win);
                }
                VK_MENU | VK_LMENU | VK_RMENU => {
                    handle_event("Alt", &event, &mut alt_last_pressed, &option.alt);
                }
                _ => {}
            }
        }
    })
}

fn handle_event(
    _label: &str,
    event: &KeyboardEvent,
    last_pressed: &mut Option<KeyboardEvent>,
    option: &PreventMenuOptionItem,
) {
    if event.is_key_up() {
        if let Some(last_pressed_event) = last_pressed.take() {
            let s = ReleasedState {
                last_pressed_event,
                released_event: *event,
            };

            if (option.should_prevent)(s) {
                if let Err(_e) = prevent_menu(option.dummy_key) {
                    #[cfg(feature = "log")]
                    log::error!("failed to prevent {} menu: {:?}", _label, _e);
                } else {
                    #[cfg(feature = "log")]
                    log::info!(
                        "prevented {} menu by sending {:?}",
                        _label,
                        option.dummy_key
                    );
                }
            } else {
                #[cfg(feature = "log")]
                log::info!("{} key released, but did not prevent menu", _label);
            }
        }
    } else {
        // key down
        last_pressed.get_or_insert(*event);
    }
}

pub fn prevent_menu(dummy_key: VIRTUAL_KEY) -> std::io::Result<()> {
    send_input(&[INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: dummy_key,
                dwFlags: KEYEVENTF_KEYUP,
                ..Default::default()
            },
        },
    }])
}

fn send_input(inputs: &[INPUT]) -> std::io::Result<()> {
    let result = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };

    if result as usize != inputs.len() {
        Err(std::io::Error::last_os_error())
    } else {
        #[cfg(feature = "log")]
        log::trace!(
            "SendInput: {:?}",
            inputs
                .iter()
                .map(|i| unsafe { i.Anonymous.ki.wVk })
                .collect::<Vec<_>>()
        );
        Ok(())
    }
}
