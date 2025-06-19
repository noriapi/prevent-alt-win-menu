use std::{fmt::Display, thread, time::Duration};

use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY,
            VK__none_, VK_LMENU, VK_LWIN, VK_MENU, VK_RMENU, VK_RWIN,
        },
        WindowsAndMessaging::{WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP},
    },
};

pub use windows::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT;

pub fn start_event_handler<
    T: MenuTriggerEvent + Clone + Send + 'static,
    I: IntoIterator<Item = T> + Send + 'static,
>(
    rx: I,
    config: Config<T>,
) -> thread::JoinHandle<()> {
    let mut handler = Handler {
        config,
        state: Default::default(),
    };

    thread::spawn(move || {
        #[cfg(feature = "log")]
        log::debug!("started event handler");

        for event in rx {
            handler.handle_keyboard_event(&event);
        }
    })
}

pub trait MenuTriggerEvent {
    fn menu_trigger(&self) -> Option<MenuTrigger>;
    fn key_state(&self) -> KeyState;
    fn is_key_down(&self) -> bool {
        matches!(self.key_state(), KeyState::Down)
    }
    fn is_key_up(&self) -> bool {
        !self.is_key_down()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuTrigger {
    Win,
    Alt,
}

impl Display for MenuTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            MenuTrigger::Win => "WIN",
            MenuTrigger::Alt => "Alt",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Down,
    Up,
}

struct Handler<T = KeyboardEvent> {
    config: Config<T>,
    state: HoldStates<T>,
}

impl<T: MenuTriggerEvent + Clone> Handler<T> {
    fn handle_keyboard_event(&mut self, event: &T) {
        if let Some((_trigger, hold)) = self.state.update(event.clone()) {
            if let Some(dummy_key) = (self.config.on_released)(hold) {
                if let Err(_e) = send_keyup(dummy_key) {
                    #[cfg(feature = "log")]
                    log::error!("failed to prevent {} menu: {:?}", _trigger, _e);
                } else {
                    #[cfg(feature = "log")]
                    log::info!("prevented {} menu by sending {:?}", _trigger, dummy_key);
                }
            } else {
                #[cfg(feature = "log")]
                log::info!("{} key released, but did not prevent menu", _trigger);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoldEvent<T = KeyboardEvent> {
    pub press: T,
    pub release: T,
}

#[derive(Debug)]
struct HoldStates<T = KeyboardEvent> {
    win: HoldState<T>,
    alt: HoldState<T>,
}

impl<T> HoldStates<T> {
    fn get_mut(&mut self, trigger: MenuTrigger) -> &mut HoldState<T> {
        match trigger {
            MenuTrigger::Win => &mut self.win,
            MenuTrigger::Alt => &mut self.alt,
        }
    }

    fn reset(&mut self) {
        self.win.reset();
        self.alt.reset();
    }
}

impl<T: MenuTriggerEvent> HoldStates<T> {
    fn update(&mut self, event: T) -> Option<(MenuTrigger, HoldEvent<T>)> {
        if let Some(trigger) = event.menu_trigger() {
            self.get_mut(trigger)
                .update(event)
                .map(|hold| (trigger, hold))
        } else {
            self.reset();
            None
        }
    }
}

impl<T> Default for HoldStates<T> {
    fn default() -> Self {
        Self {
            win: Default::default(),
            alt: Default::default(),
        }
    }
}

#[derive(Debug)]
struct HoldState<T = KeyboardEvent>(Option<T>);

impl<T> HoldState<T> {
    fn reset(&mut self) {
        self.0 = None;
    }
}

impl<T: MenuTriggerEvent> HoldState<T> {
    fn update(&mut self, event: T) -> Option<HoldEvent<T>> {
        match event.key_state() {
            KeyState::Down => {
                self.0.get_or_insert(event);
                None
            }
            KeyState::Up => self.0.take().map(|hold_start_event| HoldEvent {
                press: hold_start_event,
                release: event,
            }),
        }
    }
}

impl<T> Default for HoldState<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

pub type OnReleasedFn<T = KeyboardEvent> =
    dyn Fn(HoldEvent<T>) -> Option<VIRTUAL_KEY> + Send + Sync + 'static;

pub struct Config<T = KeyboardEvent> {
    pub on_released: Box<OnReleasedFn<T>>,
}

impl<T> Config<T> {
    pub fn set_on_released<F: Fn(HoldEvent<T>) -> Option<VIRTUAL_KEY> + Send + Sync + 'static>(
        mut self,
        f: F,
    ) -> Self {
        self.on_released = Box::new(f);
        self
    }
}

impl<T> Default for Config<T> {
    fn default() -> Self {
        Self {
            on_released: Box::new(|_| Some(VK__none_)),
        }
    }
}

pub fn send_keyup(dummy_key: VIRTUAL_KEY) -> std::io::Result<()> {
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyboardEvent {
    pub kbd: KBDLLHOOKSTRUCT,
    pub wm_key_state: WmKeyState,
}

impl KeyboardEvent {
    pub(crate) unsafe fn from_params(l_param: LPARAM, w_param: WPARAM) -> KeyboardEvent {
        let kbd = unsafe { *(l_param.0 as *const KBDLLHOOKSTRUCT) };
        let key_state = WmKeyState::from_w_param(w_param).unwrap();
        Self {
            kbd,
            wm_key_state: key_state,
        }
    }

    pub fn virtual_key(&self) -> VIRTUAL_KEY {
        VIRTUAL_KEY(self.kbd.vkCode as _)
    }

    pub fn duration_since(&self, earlier: &Self) -> Duration {
        let millis = self.kbd.time.wrapping_sub(earlier.kbd.time);
        Duration::from_millis(millis as u64)
    }
}

impl MenuTriggerEvent for KeyboardEvent {
    fn menu_trigger(&self) -> Option<crate::event_handler::MenuTrigger> {
        match self.virtual_key() {
            VK_LWIN | VK_RWIN => Some(MenuTrigger::Win),
            VK_MENU | VK_LMENU | VK_RMENU => Some(MenuTrigger::Alt),
            _ => None,
        }
    }

    fn key_state(&self) -> KeyState {
        self.wm_key_state.into()
    }
}

impl HoldEvent<KeyboardEvent> {
    pub fn duration(&self) -> Duration {
        self.release.duration_since(&self.press)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WmKeyState {
    /// [WM_KEYDOWN](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-keydown)
    KeyDown,
    /// [WM_KEYUP](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-keyup)
    KeyUp,
    /// [WM_SYSKEYDOWN](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-syskeydown)
    SysKeyDown,
    /// [WM_SYSKEYUP](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-syskeyup)
    SysKeyUp,
}

impl WmKeyState {
    fn from_w_param(w_param: WPARAM) -> Option<WmKeyState> {
        if w_param.0 == WM_KEYDOWN as usize {
            Some(WmKeyState::KeyDown)
        } else if w_param.0 == WM_KEYUP as usize {
            Some(WmKeyState::KeyUp)
        } else if w_param.0 == WM_SYSKEYDOWN as usize {
            Some(WmKeyState::SysKeyDown)
        } else if w_param.0 == WM_SYSKEYUP as usize {
            Some(WmKeyState::SysKeyUp)
        } else {
            None
        }
    }

    pub fn is_key_down(&self) -> bool {
        matches!(self, WmKeyState::KeyDown | WmKeyState::SysKeyDown)
    }

    pub fn is_key_up(&self) -> bool {
        !self.is_key_down()
    }
}

impl From<WmKeyState> for KeyState {
    fn from(value: WmKeyState) -> Self {
        match value {
            WmKeyState::KeyDown | WmKeyState::SysKeyDown => KeyState::Down,
            WmKeyState::KeyUp | WmKeyState::SysKeyUp => KeyState::Up,
        }
    }
}
