use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::repo;

#[derive(Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct Repo {
    pub id: String,
    pub path: PathBuf,
    cached: bool,
    #[serde(default)]
    pkgs: repo::PkgCache,
}

impl Repo {
    pub fn new<S: AsRef<str>>(id: S, path: S) -> Result<Repo> {
        Ok(Repo {
            id: id.as_ref().to_string(),
            path: PathBuf::from(path.as_ref()),
            ..Default::default()
        })
    }

    // TODO: build pkg cache from dir listing
    fn update_cache(&mut self) {
        self.cached = true;
    }
}

impl fmt::Display for Repo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.id, self.path.to_string_lossy())
    }
}

// TODO: fill out stub implementation
impl repo::Repo for Repo {
    fn categories(&mut self) -> Box<dyn Iterator<Item = &String> + '_> {
        if !self.cached {
            self.update_cache();
        }
        self.pkgs.categories()
    }

    fn packages<S: AsRef<str>>(&mut self, cat: S) -> Box<dyn Iterator<Item = &String> + '_> {
        if !self.cached {
            self.update_cache();
        }
        self.pkgs.packages(cat)
    }

    fn versions<S: AsRef<str>>(
        &mut self,
        cat: S,
        pkg: S,
    ) -> Box<dyn Iterator<Item = &String> + '_> {
        if !self.cached {
            self.update_cache();
        }
        self.pkgs.versions(cat, pkg)
    }

    fn from_path<S: AsRef<str>>(id: S, path: S) -> Result<Self> {
        let id = id.as_ref();
        let path = path.as_ref();
        let error: String;

        let repo_path = PathBuf::from(path);
        if repo_path.join("profiles").exists() {
            return Repo::new(id, path);
        } else {
            error = "missing profiles dir".to_string();
        }

        Err(Error::InvalidRepo {
            path: path.to_string(),
            error,
        })
    }
}
