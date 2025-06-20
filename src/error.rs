use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to register the keyboard hook")]
    HookRegistrationFailed(std::io::Error),
    #[error("the hook thread terminated unexpectedly")]
    HookThreadCrashed,
}

pub type Result<T> = std::result::Result<T, Error>;
