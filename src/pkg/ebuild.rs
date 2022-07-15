use std::collections::HashMap;
use std::io::{self, prelude::*};
use std::path::Path;
use std::{fmt, fs};

use camino::{Utf8Path, Utf8PathBuf};
use indexmap::IndexSet;
use once_cell::sync::{Lazy, OnceCell};
use regex::Regex;
use scallop::source;
use scallop::variables::string_value;
use tracing::warn;

use super::{make_pkg_traits, Package};
use crate::eapi::Key::*;
use crate::repo::ebuild::Repo;
use crate::restrict::{Restrict, Restriction};
use crate::{atom, eapi, Error, Result};

static EAPI_LINE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new("^EAPI=['\"]?(?P<EAPI>[A-Za-z0-9+_.-]*)['\"]?[\t ]*(?:#.*)?").unwrap());

#[derive(Debug, Default, Clone)]
struct Metadata<'a> {
    data: HashMap<eapi::Key, String>,
    description: OnceCell<&'a str>,
    slot: OnceCell<&'a str>,
    subslot: OnceCell<&'a str>,
    homepage: OnceCell<Vec<&'a str>>,
    keywords: OnceCell<IndexSet<&'a str>>,
    iuse: OnceCell<IndexSet<&'a str>>,
    inherit: OnceCell<IndexSet<&'a str>>,
    inherited: OnceCell<IndexSet<&'a str>>,
}

impl<'a> Metadata<'a> {
    fn new(path: &Utf8Path, eapi: &'static eapi::Eapi) -> Result<Self> {
        // TODO: run sourcing via an external process pool returning the requested variables
        source::file(path)?;
        let mut data = HashMap::new();

        // verify sourced EAPI matches parsed EAPI
        let sourced_eapi = string_value("EAPI").unwrap_or_else(|| "0".into());
        if eapi::get_eapi(&sourced_eapi)? != eapi {
            return Err(Error::InvalidValue(format!(
                "mismatched sourced and parsed EAPIs: {sourced_eapi} != {eapi}"
            )));
        }

        // required metadata variables
        for key in eapi.mandatory_keys() {
            let val = key
                .get(eapi)
                .ok_or_else(|| Error::InvalidValue(format!("missing required value: {key}")))?;
            data.insert(*key, val);
        }

        // metadata variables that default to empty
        for key in eapi.metadata_keys().difference(eapi.mandatory_keys()) {
            key.get(eapi).and_then(|v| data.insert(*key, v));
        }

        Ok(Self {
            data,
            ..Default::default()
        })
    }

    fn description(&'a self) -> &'a str {
        // mandatory key guaranteed to exist
        self.description
            .get_or_init(|| self.data.get(&Description).unwrap())
    }

    fn slot(&'a self) -> &'a str {
        self.slot.get_or_init(|| {
            // mandatory key guaranteed to exist
            let val = self.data.get(&Slot).unwrap();
            val.split_once('/').map_or(val, |x| x.0)
        })
    }

    fn subslot(&'a self) -> &'a str {
        self.subslot.get_or_init(|| {
            // mandatory key guaranteed to exist
            let val = self.data.get(&Slot).unwrap();
            val.split_once('/').map_or(val, |x| x.1)
        })
    }

    fn homepage(&'a self) -> &'a [&'a str] {
        self.homepage
            .get_or_init(|| {
                let val = self.data.get(&Homepage).map(|s| s.as_str()).unwrap_or("");
                val.split_whitespace().collect()
            })
            .as_slice()
    }

    fn keywords(&'a self) -> &'a IndexSet<&'a str> {
        self.keywords.get_or_init(|| {
            let val = self.data.get(&Keywords).map(|s| s.as_str()).unwrap_or("");
            val.split_whitespace().collect()
        })
    }

    fn iuse(&'a self) -> &'a IndexSet<&'a str> {
        self.iuse.get_or_init(|| {
            let val = self.data.get(&Iuse).map(|s| s.as_str()).unwrap_or("");
            val.split_whitespace().collect()
        })
    }

    fn inherit(&'a self) -> &'a IndexSet<&'a str> {
        self.inherit.get_or_init(|| {
            let val = self.data.get(&Inherit).map(|s| s.as_str()).unwrap_or("");
            val.split_whitespace().collect()
        })
    }

    fn inherited(&'a self) -> &'a IndexSet<&'a str> {
        self.inherited.get_or_init(|| {
            let val = self.data.get(&Inherited).map(|s| s.as_str()).unwrap_or("");
            val.split_whitespace().collect()
        })
    }
}

#[derive(Debug, Clone)]
pub struct Pkg<'a> {
    path: Utf8PathBuf,
    atom: atom::Atom,
    eapi: &'static eapi::Eapi,
    repo: &'a Repo,
    data: Metadata<'a>,
}

make_pkg_traits!(Pkg<'_>);

impl<'a> Pkg<'a> {
    pub(crate) fn new(path: &Utf8Path, repo: &'a Repo) -> Result<Self> {
        let atom = repo.atom_from_path(path)?;
        let eapi = Pkg::get_eapi(path)?;
        let data = Metadata::new(path, eapi)?;
        Ok(Pkg {
            path: path.to_path_buf(),
            atom,
            eapi,
            repo,
            data,
        })
    }

    /// Get the parsed EAPI from a given ebuild file.
    fn get_eapi<P: AsRef<Path>>(path: P) -> Result<&'static eapi::Eapi> {
        let mut eapi = &*eapi::EAPI0;
        let path = path.as_ref();
        let f = fs::File::open(path).map_err(|e| Error::IO(e.to_string()))?;
        let reader = io::BufReader::new(f);
        for line in reader.lines() {
            let line = line.map_err(|e| Error::IO(e.to_string()))?;
            match line.chars().next() {
                None | Some('#') => continue,
                _ => {
                    if let Some(c) = EAPI_LINE_RE.captures(&line) {
                        eapi = eapi::get_eapi(c.name("EAPI").unwrap().as_str())?;
                    }
                    break;
                }
            }
        }
        Ok(eapi)
    }

    /// Return a package's ebuild file path.
    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    /// Return a package's ebuild file content.
    pub fn ebuild(&self) -> Result<String> {
        fs::read_to_string(&self.path).map_err(|e| Error::IO(e.to_string()))
    }

    /// Return a package's description.
    pub fn description(&'a self) -> &'a str {
        self.data.description()
    }

    /// Return a package's slot.
    pub fn slot(&'a self) -> &'a str {
        self.data.slot()
    }

    /// Return a package's subslot.
    pub fn subslot(&'a self) -> &'a str {
        self.data.subslot()
    }

    /// Return a package's homepage.
    pub fn homepage(&'a self) -> &'a [&'a str] {
        self.data.homepage()
    }

    /// Return a package's keywords.
    pub fn keywords(&'a self) -> &'a IndexSet<&'a str> {
        self.data.keywords()
    }

    /// Return a package's IUSE.
    pub fn iuse(&'a self) -> &'a IndexSet<&'a str> {
        self.data.iuse()
    }

    /// Return the ordered set of directly inherited eclasses for a package.
    pub fn inherit(&'a self) -> &'a IndexSet<&'a str> {
        self.data.inherit()
    }

    /// Return the ordered set of inherited eclasses for a package.
    pub fn inherited(&'a self) -> &'a IndexSet<&'a str> {
        self.data.inherited()
    }
}

impl AsRef<Utf8Path> for Pkg<'_> {
    fn as_ref(&self) -> &Utf8Path {
        self.path()
    }
}

impl<'a> Package for Pkg<'a> {
    type Repo = &'a Repo;

    fn atom(&self) -> &atom::Atom {
        &self.atom
    }

    fn eapi(&self) -> &eapi::Eapi {
        self.eapi
    }

    fn repo(&self) -> Self::Repo {
        self.repo
    }
}

impl Restriction<&Pkg<'_>> for Restrict {
    fn matches(&self, pkg: &Pkg) -> bool {
        match self {
            // boolean
            Self::True => true,
            Self::False => false,

            // boolean combinations
            Self::And(vals) => vals.iter().all(|r| r.matches(pkg)),
            Self::Or(vals) => vals.iter().any(|r| r.matches(pkg)),

            Self::Atom(r) => r.matches(pkg.atom()),

            _ => {
                warn!("invalid restriction for ebuild pkg matches: {self:?}");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::Config;
    use crate::pkg::Env::*;

    #[test]
    fn test_as_ref_path() {
        fn assert_path<P: AsRef<Utf8Path>, Q: AsRef<Utf8Path>>(pkg: P, path: Q) {
            assert_eq!(pkg.as_ref(), path.as_ref());
        }

        let mut config = Config::new("pkgcraft", "", false).unwrap();
        let (t, repo) = config.temp_repo("test", 0).unwrap();
        let path = t.create_ebuild("cat/pkg-1", []).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_path(pkg, &path);
    }

    #[test]
    fn test_pkg_methods() {
        let mut config = Config::new("pkgcraft", "", false).unwrap();
        let (t, repo) = config.temp_repo("test", 0).unwrap();

        // temp repo ebuild creation defaults to the latest EAPI
        let path = t.create_ebuild("cat/pkg-1", []).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.path(), &path);
        assert!(!pkg.ebuild().unwrap().is_empty());

        let path = t.create_ebuild("cat/pkg-2", [(Eapi, "0")]).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.path(), &path);
        assert!(!pkg.ebuild().unwrap().is_empty());
    }

    #[test]
    fn test_package_trait() {
        let mut config = Config::new("pkgcraft", "", false).unwrap();
        let (t, repo) = config.temp_repo("test", 0).unwrap();
        t.create_ebuild("cat/pkg-1", []).unwrap();
        t.create_ebuild("cat/pkg-2", [(Eapi, "0")]).unwrap();

        let mut iter = repo.iter();
        let pkg1 = iter.next().unwrap();
        let pkg2 = iter.next().unwrap();

        // temp repo ebuild creation defaults to the latest EAPI
        assert_eq!(pkg1.eapi(), &*eapi::EAPI_LATEST);
        assert_eq!(pkg2.eapi(), &*eapi::EAPI0);
        assert_eq!(pkg1.atom(), &atom::cpv("cat/pkg-1").unwrap());
        assert_eq!(pkg2.atom(), &atom::cpv("cat/pkg-2").unwrap());

        // repo attribute allows recursion
        assert_eq!(pkg1.repo(), pkg2.repo());
        let mut i = pkg1.repo().iter();
        assert_eq!(pkg1, i.next().unwrap());
        assert_eq!(pkg2, i.next().unwrap());
    }

    #[test]
    fn test_pkg_env() {
        let mut config = Config::new("pkgcraft", "", false).unwrap();
        let (t, repo) = config.temp_repo("test", 0).unwrap();

        // no revision
        let path = t.create_ebuild("cat/pkg-1", []).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.env(P), "pkg-1");
        assert_eq!(pkg.env(PN), "pkg");
        assert_eq!(pkg.env(PV), "1");
        assert_eq!(pkg.env(PR), "r0");
        assert_eq!(pkg.env(PVR), "1");
        assert_eq!(pkg.env(PF), "pkg-1");
        assert_eq!(pkg.env(CATEGORY), "cat");

        // revisioned
        let path = t.create_ebuild("cat/pkg-1-r2", []).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.env(P), "pkg-1");
        assert_eq!(pkg.env(PN), "pkg");
        assert_eq!(pkg.env(PV), "1");
        assert_eq!(pkg.env(PR), "r2");
        assert_eq!(pkg.env(PVR), "1-r2");
        assert_eq!(pkg.env(PF), "pkg-1-r2");
        assert_eq!(pkg.env(CATEGORY), "cat");

        // explicit r0 revision
        let path = t.create_ebuild("cat/pkg-2-r0", []).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.env(P), "pkg-2");
        assert_eq!(pkg.env(PN), "pkg");
        assert_eq!(pkg.env(PV), "2");
        assert_eq!(pkg.env(PR), "r0");
        assert_eq!(pkg.env(PVR), "2");
        assert_eq!(pkg.env(PF), "pkg-2");
        assert_eq!(pkg.env(CATEGORY), "cat");
    }

    #[test]
    fn test_slot() {
        let mut config = Config::new("pkgcraft", "", false).unwrap();
        let (t, repo) = config.temp_repo("test", 0).unwrap();

        // default (injected by create_ebuild())
        let path = t.create_ebuild("cat/pkg-1", []).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.slot(), "0");
        assert_eq!(pkg.subslot(), "0");

        // custom lacking subslot
        let path = t.create_ebuild("cat/pkg-2", [(Slot, "1")]).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.slot(), "1");
        assert_eq!(pkg.subslot(), "1");

        // custom with subslot
        let path = t.create_ebuild("cat/pkg-3", [(Slot, "1/2")]).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.slot(), "1");
        assert_eq!(pkg.subslot(), "2");
    }

    #[test]
    fn test_description() {
        let mut config = Config::new("pkgcraft", "", false).unwrap();
        let (t, repo) = config.temp_repo("test", 0).unwrap();

        let path = t
            .create_ebuild("cat/pkg-1", [(Description, "desc")])
            .unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.description(), "desc");
    }

    #[test]
    fn test_homepage() {
        let mut config = Config::new("pkgcraft", "", false).unwrap();
        let (t, repo) = config.temp_repo("test", 0).unwrap();

        // none
        let path = t.create_ebuild("cat/pkg-1", [(Homepage, "-")]).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert!(pkg.homepage().is_empty());

        // single line
        let path = t.create_ebuild("cat/pkg-1", [(Homepage, "home")]).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.homepage(), ["home"]);

        // multiple lines
        let val = indoc::indoc! {"
            a
            b
            c
        "};
        let path = t.create_ebuild("cat/pkg-1", [(Homepage, val)]).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.homepage(), ["a", "b", "c"]);
    }

    #[test]
    fn test_keywords() {
        let mut config = Config::new("pkgcraft", "", false).unwrap();
        let (t, repo) = config.temp_repo("test", 0).unwrap();

        // none
        let path = t.create_ebuild("cat/pkg-1", []).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert!(pkg.keywords().is_empty());

        // single line
        let path = t.create_ebuild("cat/pkg-1", [(Keywords, "a b")]).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.keywords().iter().cloned().collect::<Vec<&str>>(), ["a", "b"]);

        // multiple lines
        let val = indoc::indoc! {"
            a
            b
            c
        "};
        let path = t.create_ebuild("cat/pkg-1", [(Keywords, val)]).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.keywords().iter().cloned().collect::<Vec<&str>>(), ["a", "b", "c"]);
    }

    #[test]
    fn test_iuse() {
        let mut config = Config::new("pkgcraft", "", false).unwrap();
        let (t, repo) = config.temp_repo("test", 0).unwrap();

        // none
        let path = t.create_ebuild("cat/pkg-1", []).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert!(pkg.iuse().is_empty());

        // single line
        let path = t.create_ebuild("cat/pkg-1", [(Iuse, "a b")]).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.iuse().iter().cloned().collect::<Vec<&str>>(), ["a", "b"]);

        // multiple lines
        let val = indoc::indoc! {"
            a
            b
            c
        "};
        let path = t.create_ebuild("cat/pkg-1", [(Iuse, val)]).unwrap();
        let pkg = Pkg::new(&path, &repo).unwrap();
        assert_eq!(pkg.iuse().iter().cloned().collect::<Vec<&str>>(), ["a", "b", "c"]);
    }
}
