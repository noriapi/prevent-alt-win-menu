//! Process keyboard events and suppress menu activation, providing interfaces for customization.
//!
//! This module provides mechanisms to process keyboard events and suppress menu activation,
//! along with interfaces to customize this behavior.
//!
//! If you only want to suppress the menu activation triggered by Alt or Win keys,
//! using `prevent_alt_win_menu::start` is simpler.
//! However, if you already obtain keyboard events through other means,
//! you can implement the [`MenuTriggerEvent`] trait for those event types
//! and pass them to `start_event_handler` to create a custom menu suppression mechanism.
//!
//! In other words, this module offers a flexible way to integrate with existing keyboard event sources
//! and suppress menu activation accordingly.

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

pub use windows::Win32::UI::Input::KeyboardAndMouse;

/// Starts an event-handling thread that processes each received event in a loop.
///
/// # Arguments
/// - `rx`: The source of incoming events. Must be an `IntoIterator` whose items implement [`MenuTriggerEvent`].
/// - `config`: Configuration used for event handling, such as the `on_released` callback.
///
/// # Returns
/// A [`std::thread::JoinHandle`] that represents the running event-handling thread.
///
/// This function directly spawns a thread to process events in the background.
/// It does not perform asynchronous operations.
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

/// A trait that abstracts keyboard events related to menu triggering.
///
/// By implementing this trait, you can consistently determine which key
/// triggered a menu (e.g., Alt or Win) and whether the key was pressed or released.
pub trait MenuTriggerEvent {
    /// Returns the corresponding [`MenuTrigger`] for the key event.
    ///
    /// For example, return `Some(MenuTrigger::Alt)` for `LAlt` or `RAlt`,
    /// and `Some(MenuTrigger::Win)` for `LWin` or `RWin`.
    fn menu_trigger(&self) -> Option<MenuTrigger>;

    /// Returns the current state of the key (pressed or released).
    fn key_state(&self) -> KeyState;

    /// Returns `true` if the key is currently pressed. (Default implementation provided.)
    fn is_key_down(&self) -> bool {
        matches!(self.key_state(), KeyState::Down)
    }

    /// Returns `true` if the key is currently released. (Default implementation provided.)
    fn is_key_up(&self) -> bool {
        !self.is_key_down()
    }
}

/// Indicates which modifier key was used to trigger a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuTrigger {
    /// The Windows key (either left or right).
    Win,
    /// The Alt key (either left or right).
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

/// Represents the state of a key: pressed or released.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    /// The key is currently pressed.
    Down,
    /// The key is currently released.
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

/// Represents a sequence of events where a modifier key is pressed and then released.
///
/// Typically passed to callbacks like `on_released` to determine how to handle
/// modifier key interactions.
///
/// Note: The key pressed and the key released may differ.
/// For example, consider the following sequence:
///
/// 1. `LAlt` is pressed
/// 2. `RAlt` is pressed
/// 3. `LAlt` is released
/// 4. `RAlt` is released
///
/// In this case, `press` may be `LAlt` and `release` may be `RAlt`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoldEvent<T = KeyboardEvent> {
    /// The event when the key was pressed.
    pub press: T,
    /// The event when the key was released.
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

/// A callback type invoked when a key is released.
///
/// Receives a [`HoldEvent`] and returns a virtual key code (dummy key) to send,
/// or `None` if no key should be sent.
///
/// Sending a virtual key allows Windows to treat it as a hotkey input,
/// which prevents the default menu from being displayed when Alt or Win is released.
pub type OnReleasedFn<T = KeyboardEvent> =
    dyn Fn(HoldEvent<T>) -> Option<VIRTUAL_KEY> + Send + Sync + 'static;

/// Configuration for the event handler's behavior.
///
/// Used to define how to handle a modifier key after it has been pressed and released.
/// For example, you can specify a callback to send a dummy key to prevent menu activation.
///
/// By default, it returns `Some(VK__none_)` to always suppress menu activation.
pub struct Config<T = KeyboardEvent> {
    /// A callback invoked when a key is released after being pressed.
    pub on_released: Box<OnReleasedFn<T>>,
}

impl<T> Config<T> {
    /// Sets the callback function to be invoked when a key is released.
    ///
    /// This method updates the `on_released` field with the provided function,
    /// which takes a [`HoldEvent`] representing the press and release of a modifier key.
    /// The callback should return a dummy [`VIRTUAL_KEY`] to send, or `None` if no key should be sent.
    ///
    /// # Arguments
    /// - `f`: A closure or function of type `Fn(HoldEvent<T>) -> Option<VIRTUAL_KEY>`.
    ///
    /// # Returns
    /// A modified [`Config`] instance with the new callback set (builder pattern).
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

/// Sends a key-up event for the specified virtual key code.
///
/// This function uses the Windows `SendInput` API to emit a `KEYEVENTF_KEYUP`
/// event for the given key. It is typically used to suppress system behavior
/// such as menu activation after pressing modifier keys like Alt or Win.
///
/// # Arguments
/// - `dummy_key`: The virtual key code for which to send a key-up event.
///
/// # Returns
/// Returns `Ok(())` if the event was successfully sent, or an `std::io::Error` if it failed.
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

/// Represents a single keyboard event received via a Windows low-level keyboard hook.
///
/// Internally contains the raw Windows [`KBDLLHOOKSTRUCT`] and the associated event type
/// (e.g., key down or key up).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyboardEvent {
    /// The raw Windows keyboard event structure.
    pub kbd: KBDLLHOOKSTRUCT,
    /// The raw Windows keyboard event structure.
    pub wm_key_state: WmKeyState,
}

impl KeyboardEvent {
    /// Constructs a `KeyboardEvent` from `l_param` and `w_param` inside a Windows hook procedure.
    ///
    /// # Safety
    /// `l_param` must be a valid pointer to a `KBDLLHOOKSTRUCT`.
    pub(crate) unsafe fn from_params(l_param: LPARAM, w_param: WPARAM) -> KeyboardEvent {
        let kbd = unsafe { *(l_param.0 as *const KBDLLHOOKSTRUCT) };
        let key_state = WmKeyState::from_w_param(w_param).unwrap();
        Self {
            kbd,
            wm_key_state: key_state,
        }
    }

    /// Returns the virtual key code of the event.
    pub fn virtual_key(&self) -> VIRTUAL_KEY {
        VIRTUAL_KEY(self.kbd.vkCode as _)
    }

    /// Returns the duration elapsed since the given earlier event.
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
    /// Returns the duration between the key press and release.
    pub fn duration(&self) -> Duration {
        self.release.duration_since(&self.press)
    }
}

/// Represents the type of Windows message related to a keyboard event.
///
/// See also: [Keyboard Input](https://learn.microsoft.com/en-us/windows/win32/inputdev/about-keyboard-input#keystroke-messages)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WmKeyState {
    /// [`WM_KEYDOWN`](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-keydown)
    KeyDown,
    /// [`WM_KEYUP`](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-keyup)
    KeyUp,
    /// [`WM_SYSKEYDOWN`](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-syskeydown)
    SysKeyDown,
    /// [`WM_SYSKEYUP`](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-syskeyup)
    SysKeyUp,
}

impl WmKeyState {
    /// Converts a `w_param` to the corresponding `WmKeyState`, if applicable.
    ///
    /// Returns `None` if the value does not match a known key message.
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

    /// Returns `true` if this is a key-down event.
    pub fn is_key_down(&self) -> bool {
        KeyState::from(*self) == KeyState::Down
    }

    /// Returns `true` if this is a key-up event.
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
