//! KoboldCpp settings: option table, `.kcpps`/`.kcppt` files and the command line view.

pub mod archs;
pub mod args;
pub mod cli;
pub mod kcpps;

pub use kcpps::{Backend, ConfigError, Kcpps};
