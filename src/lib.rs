#![warn(unreachable_pub)]

pub(crate) mod archive;
pub mod atom;
pub(crate) mod command;
pub mod config;
pub mod depspec;
pub mod eapi;
mod error;
pub(crate) mod files;
mod macros;
pub mod peg;
pub mod pkg;
pub mod pkgsh;
pub mod repo;
pub mod restrict;
mod sync;
#[cfg(test)]
pub(crate) mod test;
pub mod utils;

pub use self::error::{Error, Result};
