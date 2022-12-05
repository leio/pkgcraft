use std::cmp::Ordering;
use std::fmt::{self, Write};
use std::hash::Hash;
use std::str::FromStr;

use cached::{proc_macro::cached, SizedCache};
use itertools::Itertools;

pub use self::version::Version;
use self::version::{Operator, ParsedVersion};
use crate::eapi::{IntoEapi, EAPI_PKGCRAFT};
use crate::macros::cmp_not_equal;
use crate::orderedset::OrderedSet;
use crate::Error;

// re-export Restrict
pub use restrict::Restrict;

pub mod parse;
pub mod restrict;
pub(crate) mod version;

#[repr(C)]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub enum Blocker {
    Strong, // !!cat/pkg
    Weak,   // !cat/pkg
}

impl fmt::Display for Blocker {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Blocker::Weak => write!(f, "!"),
            Blocker::Strong => write!(f, "!!"),
        }
    }
}

impl FromStr for Blocker {
    type Err = Error;

    fn from_str(s: &str) -> crate::Result<Self> {
        match s {
            "!!" => Ok(Self::Strong),
            "!" => Ok(Self::Weak),
            _ => Err(Error::InvalidValue(format!("invalid blocker: {s}"))),
        }
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub enum SlotOperator {
    Equal,
    Star,
}

impl fmt::Display for SlotOperator {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Equal => write!(f, "="),
            Self::Star => write!(f, "*"),
        }
    }
}

impl FromStr for SlotOperator {
    type Err = Error;

    fn from_str(s: &str) -> crate::Result<Self> {
        match s {
            "=" => Ok(Self::Equal),
            "*" => Ok(Self::Star),
            _ => Err(Error::InvalidValue(format!("invalid slot operator: {s}"))),
        }
    }
}

/// Parsed package atom from borrowed input string
#[derive(Debug, Default)]
pub(crate) struct ParsedAtom<'a> {
    pub(crate) category: &'a str,
    pub(crate) package: &'a str,
    pub(crate) blocker: Option<Blocker>,
    pub(crate) version: Option<ParsedVersion<'a>>,
    pub(crate) version_str: Option<&'a str>,
    pub(crate) slot: Option<&'a str>,
    pub(crate) subslot: Option<&'a str>,
    pub(crate) slot_op: Option<SlotOperator>,
    pub(crate) use_deps: Option<Vec<&'a str>>,
    pub(crate) repo: Option<&'a str>,
}

impl ParsedAtom<'_> {
    pub(crate) fn into_owned(self) -> crate::Result<Atom> {
        let version = match (self.version, self.version_str) {
            (Some(v), Some(s)) => Some(v.into_owned(s)?),
            _ => None,
        };

        Ok(Atom {
            category: self.category.to_string(),
            package: self.package.to_string(),
            blocker: self.blocker,
            version,
            slot: self.slot.map(|s| s.to_string()),
            subslot: self.subslot.map(|s| s.to_string()),
            slot_op: self.slot_op,
            use_deps: self
                .use_deps
                .as_ref()
                .map(|u| u.iter().map(|s| s.to_string()).collect()),
            repo: self.repo.map(|s| s.to_string()),
        })
    }
}

/// Package atom
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Atom {
    category: String,
    package: String,
    blocker: Option<Blocker>,
    version: Option<Version>,
    slot: Option<String>,
    subslot: Option<String>,
    slot_op: Option<SlotOperator>,
    use_deps: Option<OrderedSet<String>>,
    repo: Option<String>,
}

#[cached(
    type = "SizedCache<String, crate::Result<Atom>>",
    create = "{ SizedCache::with_size(1000) }",
    convert = r#"{ s.to_string() }"#
)]
/// Create a new Atom from a given CPV string (e.g. cat/pkg-1).
pub fn cpv(s: &str) -> crate::Result<Atom> {
    let mut atom = parse::cpv(s)?;
    atom.version_str = Some(s);
    atom.into_owned()
}

impl Atom {
    /// Verify a string represents a valid atom.
    pub fn valid<E: IntoEapi>(s: &str, eapi: E) -> crate::Result<()> {
        parse::dep_str(s, eapi.into_eapi()?)?;
        Ok(())
    }

    /// Verify a string represents a valid atom.
    pub fn valid_cpv(s: &str) -> crate::Result<()> {
        parse::cpv(s)?;
        Ok(())
    }

    /// Create a new Atom from a given string.
    pub fn new<E: IntoEapi>(s: &str, eapi: E) -> crate::Result<Self> {
        parse::dep(s, eapi.into_eapi()?)
    }

    /// Return an atom's category.
    pub fn category(&self) -> &str {
        &self.category
    }

    /// Return an atom's package.
    pub fn package(&self) -> &str {
        &self.package
    }

    /// Return an atom's blocker.
    pub fn blocker(&self) -> Option<Blocker> {
        self.blocker
    }

    /// Return an atom's USE flag dependencies.
    pub fn use_deps(&self) -> Option<&OrderedSet<String>> {
        self.use_deps.as_ref()
    }

    /// Return an atom's version.
    pub fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }

    /// Return an atom's revision.
    pub fn revision(&self) -> Option<&version::Revision> {
        self.version.as_ref().map(|v| v.revision())
    }

    /// Return an atom's CAT/PN value, e.g. `>=cat/pkg-1-r2:3` -> `cat/pkg`.
    pub fn key(&self) -> String {
        format!("{}/{}", self.category, self.package)
    }

    /// Return an atom's CPV, e.g. `>=cat/pkg-1-r2:3` -> `cat/pkg-1-r2`.
    pub fn cpv(&self) -> String {
        match &self.version {
            Some(ver) => format!("{}/{}-{ver}", self.category, self.package),
            None => format!("{}/{}", self.category, self.package),
        }
    }

    /// Return an atom's slot.
    pub fn slot(&self) -> Option<&str> {
        self.slot.as_deref()
    }

    /// Return an atom's subslot.
    pub fn subslot(&self) -> Option<&str> {
        self.subslot.as_deref()
    }

    /// Return an atom's slot operator.
    pub fn slot_op(&self) -> Option<SlotOperator> {
        self.slot_op
    }

    /// Return an atom's repository.
    pub fn repo(&self) -> Option<&str> {
        self.repo.as_deref()
    }
}

impl fmt::Display for Atom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut s = String::new();

        // append blocker
        if let Some(blocker) = self.blocker {
            write!(s, "{}", blocker)?;
        }

        // append version operator with cpv
        let cpv = self.cpv();
        match self.version.as_ref().and_then(|v| v.op()) {
            Some(Operator::Less) => write!(s, "<{cpv}")?,
            Some(Operator::LessOrEqual) => write!(s, "<={cpv}")?,
            Some(Operator::Equal) => write!(s, "={cpv}")?,
            Some(Operator::EqualGlob) => write!(s, "={cpv}*")?,
            Some(Operator::Approximate) => write!(s, "~{cpv}")?,
            Some(Operator::GreaterOrEqual) => write!(s, ">={cpv}")?,
            Some(Operator::Greater) => write!(s, ">{cpv}")?,
            None => s.push_str(&cpv),
        }

        // append slot data
        match (self.slot(), self.subslot(), self.slot_op()) {
            (Some(slot), Some(subslot), Some(op)) => write!(s, ":{slot}/{subslot}{op}")?,
            (Some(slot), Some(subslot), None) => write!(s, ":{slot}/{subslot}")?,
            (Some(slot), None, Some(op)) => write!(s, ":{slot}{op}")?,
            (Some(x), None, None) => write!(s, ":{x}")?,
            (None, None, Some(x)) => write!(s, ":{x}")?,
            _ => (),
        }

        // append use deps
        if let Some(x) = &self.use_deps {
            write!(s, "[{}]", x.iter().join(","))?;
        }

        // append repo
        if let Some(repo) = &self.repo {
            write!(s, "::{repo}")?;
        }

        write!(f, "{s}")
    }
}

impl Ord for Atom {
    fn cmp(&self, other: &Self) -> Ordering {
        cmp_not_equal!(&self.category, &other.category);
        cmp_not_equal!(&self.package, &other.package);
        cmp_not_equal!(&self.version, &other.version);
        cmp_not_equal!(&self.blocker, &other.blocker);
        cmp_not_equal!(&self.slot, &other.slot);
        cmp_not_equal!(&self.subslot, &other.subslot);
        cmp_not_equal!(&self.slot_op, &other.slot_op);
        cmp_not_equal!(&self.use_deps, &other.use_deps);
        self.repo.cmp(&other.repo)
    }
}

impl PartialOrd for Atom {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl FromStr for Atom {
    type Err = Error;

    fn from_str(s: &str) -> crate::Result<Self> {
        Atom::new(s, &*EAPI_PKGCRAFT)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::test::{AtomData, VersionData};

    use super::*;

    #[test]
    fn test_fmt() {
        let mut atom: Atom;
        for s in [
            "cat/pkg",
            "<cat/pkg-4",
            "<=cat/pkg-4-r1",
            "=cat/pkg-4-r0",
            "=cat/pkg-4-r01",
            "=cat/pkg-4*",
            "~cat/pkg-4",
            ">=cat/pkg-r1-2-r3",
            ">cat/pkg-4-r1:0=",
            ">cat/pkg-4-r1:0/2=[use]",
            ">cat/pkg-4-r1:0/2=[use]::repo",
            "!cat/pkg",
            "!!<cat/pkg-4",
        ] {
            atom = Atom::from_str(&s).unwrap();
            assert_eq!(format!("{atom}"), s);
        }
    }

    #[test]
    fn test_atom_key() {
        let mut atom: Atom;
        for (s, key) in [
            ("cat/pkg", "cat/pkg"),
            ("<cat/pkg-4", "cat/pkg"),
            ("<=cat/pkg-4-r1", "cat/pkg"),
            ("=cat/pkg-4", "cat/pkg"),
            ("=cat/pkg-4*", "cat/pkg"),
            ("~cat/pkg-4", "cat/pkg"),
            (">=cat/pkg-r1-2-r3", "cat/pkg-r1"),
            (">cat/pkg-4-r1:0=", "cat/pkg"),
        ] {
            atom = Atom::from_str(&s).unwrap();
            assert_eq!(atom.key(), key);
        }
    }

    #[test]
    fn test_atom_version() {
        let mut atom: Atom;
        for (s, version) in [
            ("cat/pkg", None),
            ("<cat/pkg-4", Some("<4")),
            ("<=cat/pkg-4-r1", Some("<=4-r1")),
            ("=cat/pkg-4", Some("=4")),
            ("=cat/pkg-4*", Some("=4*")),
            ("~cat/pkg-4", Some("~4")),
            (">=cat/pkg-r1-2-r3", Some(">=2-r3")),
            (">cat/pkg-4-r1:0=", Some(">4-r1")),
        ] {
            atom = Atom::from_str(&s).unwrap();
            let version = version.map(|s| parse::version_with_op(s).unwrap());
            assert_eq!(atom.version(), version.as_ref());
        }
    }

    #[test]
    fn test_atom_revision() {
        let mut atom: Atom;
        for (s, revision) in [
            ("cat/pkg", None),
            ("<cat/pkg-4", Some("0")),
            ("<=cat/pkg-4-r1", Some("1")),
            (">=cat/pkg-r1-2-r3", Some("3")),
            (">cat/pkg-4-r1:0=", Some("1")),
        ] {
            atom = Atom::from_str(&s).unwrap();
            let revision = revision.map(|s| version::Revision::from_str(s).unwrap());
            assert_eq!(atom.revision(), revision.as_ref(), "{s} failed");
        }
    }

    #[test]
    fn test_atom_cpv() {
        let mut atom: Atom;
        for (s, cpv) in [
            ("cat/pkg", "cat/pkg"),
            ("<cat/pkg-4", "cat/pkg-4"),
            ("<=cat/pkg-4-r1", "cat/pkg-4-r1"),
            ("=cat/pkg-4", "cat/pkg-4"),
            ("=cat/pkg-4*", "cat/pkg-4"),
            ("~cat/pkg-4", "cat/pkg-4"),
            (">=cat/pkg-r1-2-r3", "cat/pkg-r1-2-r3"),
            (">cat/pkg-4-r1:0=", "cat/pkg-4-r1"),
        ] {
            atom = Atom::from_str(&s).unwrap();
            assert_eq!(atom.cpv(), cpv);
        }
    }

    #[test]
    fn test_sorting() {
        let data = AtomData::load().unwrap();
        for (unsorted, expected) in data.sorting.iter() {
            let mut atoms: Vec<_> = unsorted
                .iter()
                .map(|s| Atom::from_str(s).unwrap())
                .collect();
            atoms.sort();
            let sorted: Vec<_> = atoms.iter().map(|x| format!("{x}")).collect();
            assert_eq!(&sorted, expected);
        }
    }

    #[test]
    fn test_hashing() {
        let data = VersionData::load().unwrap();
        for (versions, size) in data.hashing.iter() {
            let set: HashSet<_> = versions
                .iter()
                .map(|s| Atom::from_str(&format!("=cat/pkg-{s}")).unwrap())
                .collect();
            assert_eq!(set.len(), *size);
        }
    }
}
