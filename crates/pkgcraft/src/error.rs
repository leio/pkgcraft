use std::convert::Infallible;

use crate::peg;
use crate::pkg::Package;

/// A `Result` alias where the `Err` case is `pkgcraft::Error`.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    PegParse(peg::Error),
    #[error("config error: {0}")]
    Config(String),
    #[error("{0}")]
    InvalidValue(String),
    #[error("invalid repo: {id}: {err}")]
    InvalidRepo { id: String, err: String },
    #[error("invalid pkg: {id}: {err}")]
    InvalidPkg { id: String, err: String },
    #[error("{id}: {err}")]
    Pkg { id: String, err: String },
    #[error("{0}")]
    IO(String),
    #[error("{0}")]
    Overflow(String),
    #[error("{0}")]
    Pkgsh(#[from] scallop::Error),
    #[error("{0}")]
    RepoInit(String),
    #[error("failed syncing repo: {0}")]
    RepoSync(String),
    #[error("timed out: {0}")]
    Timeout(String),
}

impl From<Error> for scallop::Error {
    fn from(e: Error) -> Self {
        scallop::Error::Base(e.to_string())
    }
}

// Stub for infallible From<T> conversion types.
// TODO: This should be able to be dropped when upstream stabilizes:
// https://github.com/rust-lang/rust/issues/64715.
impl From<Infallible> for Error {
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}

pub(crate) trait PackageError: Package {
    fn invalid_pkg_err<E: std::error::Error>(&self, err: E) -> Error {
        Error::InvalidPkg {
            id: self.to_string(),
            err: err.to_string(),
        }
    }

    fn pkg_err<E: std::error::Error>(&self, err: E) -> Error {
        Error::Pkg {
            id: self.to_string(),
            err: err.to_string(),
        }
    }
}
