# prevent-alt-win-menu

[![github](https://img.shields.io/badge/repository-noriapi?style=for-the-badge&logo=github&label=github&labelColor=555555)](https://github.com/noriapi/prevent-alt-win-menu)
[![Crates.io Version](https://img.shields.io/crates/v/prevent-alt-win-menu?style=for-the-badge&logo=rust)](https://crates.io/crates/prevent-alt-win-menu)
[![docs.rs](https://img.shields.io/docsrs/prevent-alt-win-menu?style=for-the-badge&logo=docs.rs&label=docs.rs)](https://docs.rs/prevent-alt-win-menu)

Prevents the menu bar or Start menu from appearing when the Alt or Windows key is
released on Windows.

## Overview

On Windows, releasing the `Alt` key typically activates the menu bar of the focused
window (if it has one), and releasing the `Windows` key opens the Start menu.
This crate allows you to **suppress** these behaviors, which is useful for apps with
custom global keyboard handling or immersive fullscreen UIs.

## Platform

- Windows only

## Quick Start

Call [`start`] at the beginning of your application. You do **not** need to hold on
to the returned [`JoinHandles`] unless you explicitly want to `join()` the threads
or detect their termination.

```rust,no_run
use prevent_alt_win_menu::event_handler::Config;
use prevent_alt_win_menu::start;

// Starts the suppression logic in background threads
let _ = start(Config::default()).expect("failed to start menu suppression");
```

## How it works

This crate installs a low-level keyboard hook using `SetWindowsHookExW` and listens
for `WM_KEYUP` events of:

- `VK_MENU` / `VK_LMENU` / `VK_RMENU` (Alt key)

- `VK_LWIN` / `VK_RWIN` (Left/Right Windows key)

When such a key is released, a dummy key-up event (by default, `VK__none_`) is programmatically
sent immediately.
This causes Windows to interpret the input as a hotkey sequence rather than a standalone
key release — effectively suppressing the default menu activation behavior.

## Configuration

- _Custom dummy key_: You can specify any virtual key code to be used as the dummy
  key.

- _Conditional suppression_: A callback function allows you to decide at runtime
  whether or not to send the dummy key, based on the released key or app state.

## Limitations

- May interfere with other hooks that rely on raw `Alt` or `Win` key events.

## License

MIT OR Apache-2.0
