pub mod error;
pub mod install;
pub mod release;
pub mod service;
pub mod signature;
pub mod source;
pub mod stage;
#[cfg(test)]
mod testing;
pub mod version;

pub use error::{Error, Result};
pub use install::Target;
pub use service::{Config, Relaunch, Updater, spawn};
pub use source::{GitHub, Source};
