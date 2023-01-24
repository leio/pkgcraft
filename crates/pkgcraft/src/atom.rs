use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use strum::{AsRefStr, Display, EnumString};

use self::version::ParsedVersion;
pub use self::version::{Revision, Version};
use crate::eapi::{IntoEapi, EAPI_PKGCRAFT};
use crate::macros::bool_not_equal;
use crate::set::OrderedSet;
use crate::Error;

pub mod parse;
pub(crate) mod version;

#[repr(C)]
#[derive(
    AsRefStr, Display, EnumString, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone,
)]
pub enum Blocker {
    #[strum(serialize = "!!")]
    Strong = 1,
    #[strum(serialize = "!")]
    Weak,
}

#[repr(C)]
#[derive(
    AsRefStr, Display, EnumString, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone,
)]
pub enum SlotOperator {
    #[strum(serialize = "=")]
    Equal = 1,
    #[strum(serialize = "*")]
    Star,
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
    pub(crate) fn into_owned(self, s: &str) -> crate::Result<Atom> {
        let version = match (self.version, self.version_str) {
            (Some(v), Some(vs)) => Some(v.into_owned(vs)?),
            _ => None,
        };

        Ok(Atom {
            full: s.to_string(),
            category: self.category.to_string(),
            package: self.package.to_string(),
            blocker: self.blocker,
            version,
            slot: self.slot.map(|s| s.to_string()),
            subslot: self.subslot.map(|s| s.to_string()),
            slot_op: self.slot_op,
            use_deps: self.use_deps.map(|u| {
                // sort use deps by the first letter or number
                let mut set = OrderedSet::from_iter(u.iter().map(|s| s.to_string()));
                let f = |c: &char| c >= &'0';
                set.sort_by(|u1, u2| u1.chars().find(f).cmp(&u2.chars().find(f)));
                set
            }),
            repo: self.repo.map(|s| s.to_string()),
        })
    }
}

/// Package atom
#[derive(Debug, Clone)]
pub struct Atom {
    full: String,
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

impl PartialEq for Atom {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Atom {}

impl Hash for Atom {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key().hash(state);
    }
}

/// Key type used for implementing various traits, e.g. Eq, Hash, etc.
type AtomKey<'a> = (
    &'a str,
    &'a str,
    Option<&'a Version>,
    Option<Blocker>,
    Option<&'a str>,
    Option<&'a str>,
    Option<SlotOperator>,
    Option<&'a OrderedSet<String>>,
    Option<&'a str>,
);

impl Atom {
    /// Verify a string represents a valid atom.
    pub fn valid<E: IntoEapi>(s: &str, eapi: E) -> crate::Result<()> {
        parse::dep_str(s, eapi.into_eapi()?)?;
        Ok(())
    }

    /// Verify a string represents a valid atom.
    pub fn valid_cpv(s: &str) -> crate::Result<()> {
        parse::cpv_str(s)?;
        Ok(())
    }

    /// Create a new Atom from a given string.
    pub fn new<E: IntoEapi>(s: &str, eapi: E) -> crate::Result<Self> {
        parse::dep(s, eapi.into_eapi()?)
    }

    /// Create a new Atom from a given CPV string (e.g. cat/pkg-1).
    pub fn new_cpv(s: &str) -> crate::Result<Self> {
        parse::cpv(s)
    }

    /// Determine if two atoms intersect ignoring blockers.
    pub fn intersects(&self, other: &Self) -> bool {
        bool_not_equal!(&self.category(), &other.category());
        bool_not_equal!(&self.package(), &other.package());

        if let (Some(x), Some(y)) = (self.slot(), other.slot()) {
            bool_not_equal!(x, y);
        }

        if let (Some(x), Some(y)) = (self.subslot(), other.subslot()) {
            bool_not_equal!(x, y);
        }

        if let (Some(x), Some(y)) = (self.use_deps(), other.use_deps()) {
            let flags: HashSet<_> = x.symmetric_difference(y).map(|s| s.as_str()).collect();
            for f in &flags {
                if f.starts_with('-') && flags.contains(&f[1..]) {
                    return false;
                }
            }
        }

        if let (Some(x), Some(y)) = (self.repo(), other.repo()) {
            bool_not_equal!(x, y);
        }

        match (self.version(), other.version()) {
            (Some(x), Some(y)) => x.intersects(y),
            (None, _) | (_, None) => true,
        }
    }

    /// Return an atom's string value.
    pub fn as_str(&self) -> &str {
        &self.full
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
    pub fn revision(&self) -> Option<&Revision> {
        self.version.as_ref().map(|v| v.revision())
    }

    /// Return an atom's CAT/PN value, e.g. `>=cat/pkg-1-r2:3` -> `cat/pkg`.
    pub fn cpn(&self) -> String {
        format!("{}/{}", self.category, self.package)
    }

    /// Return an atom's CPV, e.g. `>=cat/pkg-1-r2:3` -> `cat/pkg-1-r2`.
    pub fn cpv(&self) -> String {
        match &self.version {
            Some(ver) => format!("{}/{}-{ver}", self.category, self.package),
            None => self.cpn(),
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

    /// Return a key value used to implement various traits, e.g. Eq, Hash, etc.
    fn key(&self) -> AtomKey {
        (
            self.category(),
            self.package(),
            self.version(),
            self.blocker(),
            self.slot(),
            self.subslot(),
            self.slot_op(),
            self.use_deps(),
            self.repo(),
        )
    }
}

impl fmt::Display for Atom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.full)
    }
}

impl Ord for Atom {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
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
    use std::collections::{HashMap, HashSet};

    use itertools::Itertools;

    use crate::test::{AtomData, VersionData};
    use crate::utils::hash;

    use super::*;

    // Convert an Atom to a string based on its components. This isn't used for fmt::Display
    // because it doesn't always preserve the original string since the use flag component sorts,
    // and collapse similar elements on creation.
    fn atom_to_string(atom: &Atom) -> String {
        let mut s = String::new();

        // append blocker
        if let Some(blocker) = atom.blocker {
            s.push_str(&format!("{blocker}"));
        }

        // append version operator with cpv
        let cpv = atom.cpv();
        use version::Operator::*;
        match atom.version.as_ref().and_then(|v| v.op()) {
            Some(Less) => s.push_str(&format!("<{cpv}")),
            Some(LessOrEqual) => s.push_str(&format!("<={cpv}")),
            Some(Equal) => s.push_str(&format!("={cpv}")),
            Some(EqualGlob) => s.push_str(&format!("={cpv}*")),
            Some(Approximate) => s.push_str(&format!("~{cpv}")),
            Some(GreaterOrEqual) => s.push_str(&format!(">={cpv}")),
            Some(Greater) => s.push_str(&format!(">{cpv}")),
            None => s.push_str(&cpv),
        }

        // append slot data
        match (atom.slot(), atom.subslot(), atom.slot_op()) {
            (Some(slot), Some(subslot), Some(op)) => s.push_str(&format!(":{slot}/{subslot}{op}")),
            (Some(slot), Some(subslot), None) => s.push_str(&format!(":{slot}/{subslot}")),
            (Some(slot), None, Some(op)) => s.push_str(&format!(":{slot}{op}")),
            (Some(x), None, None) => s.push_str(&format!(":{x}")),
            (None, None, Some(x)) => s.push_str(&format!(":{x}")),
            _ => (),
        }

        // append use deps
        if let Some(x) = &atom.use_deps {
            s.push_str(&format!("[{}]", x.iter().join(",")));
        }

        // append repo
        if let Some(repo) = &atom.repo {
            s.push_str(&format!("::{repo}"));
        }

        s
    }

    #[test]
    fn test_to_string() {
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
            "cat/pkg[u,u]",
            "cat/pkg[b,a]",
        ] {
            atom = Atom::from_str(s).unwrap();
            assert_eq!(atom.to_string(), s);
        }
    }

    #[test]
    fn test_atom_cpn() {
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
            atom = Atom::from_str(s).unwrap();
            assert_eq!(atom.cpn(), key);
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
            atom = Atom::from_str(s).unwrap();
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
            atom = Atom::from_str(s).unwrap();
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
            atom = Atom::from_str(s).unwrap();
            assert_eq!(atom.cpv(), cpv);
        }
    }

    #[test]
    fn test_cmp() {
        let op_map: HashMap<&str, Ordering> =
            [("<", Ordering::Less), ("==", Ordering::Equal), (">", Ordering::Greater)]
                .into_iter()
                .collect();

        let data = AtomData::load().unwrap();
        for (expr, (s1, op, s2)) in data.compares() {
            let a1 = Atom::from_str(s1).unwrap();
            let a2 = Atom::from_str(s2).unwrap();
            match op {
                "!=" => {
                    assert_ne!(a1, a2, "failed comparing {expr}");
                    assert_ne!(a2, a1, "failed comparing {expr}");
                }
                _ => {
                    let op = op_map[op];
                    assert_eq!(a1.cmp(&a2), op, "failed comparing {expr}");
                    assert_eq!(a2.cmp(&a1), op.reverse(), "failed comparing {expr}");

                    // verify the following property holds since both Hash and Eq are implemented:
                    // k1 == k2 -> hash(k1) == hash(k2)
                    if op == Ordering::Equal {
                        assert_eq!(hash(a1), hash(a2), "failed hash {expr}");
                    }
                }
            }
        }
    }

    #[test]
    fn test_intersects() {
        // convert string to CPV falling back to regular atom
        let parse = |s: &str| -> Atom { Atom::new_cpv(s).or_else(|_| Atom::from_str(s)).unwrap() };

        // convert string to non-op version falling back to op-ed version
        let ver_parse = |s: &str| -> Version {
            Version::new(s)
                .or_else(|_| Version::new_with_op(s))
                .unwrap()
        };

        // inject version intersects data from version.toml into Atom objects
        let data = VersionData::load().unwrap();
        let a = Atom::from_str("a/b").unwrap();
        for d in data.intersects {
            // test intersections between all pairs of distinct values
            for vals in d.vals.iter().map(|s| s.as_str()).permutations(2) {
                let (mut a1, mut a2) = (a.clone(), a.clone());
                a1.version = Some(ver_parse(vals[0]));
                a2.version = Some(ver_parse(vals[1]));
                let (s1, s2) = (atom_to_string(&a1), atom_to_string(&a2));

                // elements intersect themselves
                assert!(a1.intersects(&a1), "{s1} doesn't intersect {s1}");
                assert!(a2.intersects(&a2), "{s2} doesn't intersect {s2}");

                // intersects depending on status
                match d.status {
                    true => assert!(a1.intersects(&a2), "{s1} doesn't intersect {s2}"),
                    false => assert!(!a1.intersects(&a2), "{s1} intersects {s2}"),
                }
            }
        }

        let data = AtomData::load().unwrap();
        for d in data.intersects {
            // test intersections between all pairs of distinct values
            for vals in d.vals.iter().map(|s| s.as_str()).permutations(2) {
                let (s1, s2) = (vals[0], vals[1]);
                let (a1, a2) = (parse(s1), parse(s2));

                // elements intersect themselves
                assert!(a1.intersects(&a1), "{s1} doesn't intersect {s1}");
                assert!(a2.intersects(&a2), "{s2} doesn't intersect {s2}");

                // intersects depending on status
                match d.status {
                    true => assert!(a1.intersects(&a2), "{s1} doesn't intersect {s2}"),
                    false => assert!(!a1.intersects(&a2), "{s1} intersects {s2}"),
                }
            }
        }
    }

    #[test]
    fn test_sorting() {
        let data = AtomData::load().unwrap();
        for d in data.sorting {
            let mut reversed: Vec<_> = d
                .sorted
                .iter()
                .map(|s| Atom::from_str(s).unwrap())
                .rev()
                .collect();
            reversed.sort();
            let mut sorted: Vec<_> = reversed.iter().map(|x| x.to_string()).collect();
            if d.equal {
                // equal atoms aren't sorted so reversing should restore the original order
                sorted = sorted.into_iter().rev().collect();
            }
            assert_eq!(&sorted, &d.sorted);
        }
    }

    #[test]
    fn test_hashing() {
        let data = VersionData::load().unwrap();
        for d in data.hashing {
            let set: HashSet<_> = d
                .versions
                .iter()
                .map(|s| Atom::from_str(&format!("=cat/pkg-{s}")).unwrap())
                .collect();
            if d.equal {
                assert_eq!(set.len(), 1);
            } else {
                assert_eq!(set.len(), d.versions.len());
            }
        }
    }
}
