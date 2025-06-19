use thiserror::Error;
pub use windows::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT;

#[derive(Debug, Error)]
pub enum Error {
    #[error("keyboard hook is already initialized")]
    AlreadyInitialized,
    #[error("windows error")]
    Windows(#[from] windows::core::Error),
    #[error("io error")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
