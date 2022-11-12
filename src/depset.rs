use std::collections::VecDeque;
use std::fmt;

use itertools::Itertools;

use crate::atom::{Atom, Restrict as AtomRestrict};
use crate::eapi::{Eapi, Feature};
use crate::macros::extend_left;
use crate::restrict::{self, Restriction, Str};

/// Uri object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uri {
    uri: String,
    rename: Option<String>,
}

impl Uri {
    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn rename(&self) -> Option<&str> {
        self.rename.as_deref()
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.uri)?;
        if let Some(s) = &self.rename {
            write!(f, " -> {s}")?;
        }
        Ok(())
    }
}

impl AsRef<str> for Uri {
    fn as_ref(&self) -> &str {
        &self.uri
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepSet<T> {
    deps: Vec<DepRestrict<T>>,
}

impl<T: fmt::Display> fmt::Display for DepSet<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.deps.iter().map(|x| x.to_string()).join(" "))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepRestrict<T> {
    Matches(T, bool),
    // logic conditionals
    AllOf(Vec<Box<DepRestrict<T>>>),
    AnyOf(Vec<Box<DepRestrict<T>>>),
    ExactlyOneOf(Vec<Box<DepRestrict<T>>>), // REQUIRED_USE only
    AtMostOneOf(Vec<Box<DepRestrict<T>>>),  // REQUIRED_USE only
    UseEnabled(String, Vec<Box<DepRestrict<T>>>),
    UseDisabled(String, Vec<Box<DepRestrict<T>>>),
}

impl<T> DepSet<T> {
    pub fn flatten(&self) -> DepSetFlatten<T> {
        DepSetFlatten {
            deps: self.deps.iter().collect(),
        }
    }
}

impl<T: fmt::Display> fmt::Display for DepRestrict<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let p = |args: &[Box<DepRestrict<T>>]| -> String {
            args.iter().map(|x| x.to_string()).join(" ")
        };

        match self {
            Self::Matches(val, true) => write!(f, "{val}"),
            Self::Matches(val, false) => write!(f, "!{val}"),
            Self::AllOf(vals) => write!(f, "( {} )", p(vals)),
            Self::AnyOf(vals) => write!(f, "|| ( {} )", p(vals)),
            Self::ExactlyOneOf(vals) => write!(f, "^^ ( {} )", p(vals)),
            Self::AtMostOneOf(vals) => write!(f, "?? ( {} )", p(vals)),
            Self::UseEnabled(s, vals) => write!(f, "{s}? ( {} )", p(vals)),
            Self::UseDisabled(s, vals) => write!(f, "!{s}? ( {} )", p(vals)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Restrict<T> {
    Any(T),
}

impl Restriction<&DepSet<Atom>> for Restrict<AtomRestrict> {
    fn matches(&self, val: &DepSet<Atom>) -> bool {
        match self {
            Self::Any(r) => val.flatten().any(|v| r.matches(v)),
        }
    }
}

impl Restriction<&DepSet<String>> for Restrict<Str> {
    fn matches(&self, val: &DepSet<String>) -> bool {
        match self {
            Self::Any(r) => val.flatten().any(|v| r.matches(v)),
        }
    }
}

impl Restriction<&DepSet<Uri>> for Restrict<Str> {
    fn matches(&self, val: &DepSet<Uri>) -> bool {
        match self {
            Self::Any(r) => val.flatten().any(|v| r.matches(v.as_ref())),
        }
    }
}

#[derive(Debug)]
pub struct DepSetFlatten<'a, T> {
    deps: VecDeque<&'a DepRestrict<T>>,
}

impl<'a, T: fmt::Debug> Iterator for DepSetFlatten<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        use DepRestrict::*;
        loop {
            match self.deps.pop_front() {
                Some(Matches(val, _)) => return Some(val),
                Some(AllOf(vals)) => extend_left!(self.deps, vals.iter().map(AsRef::as_ref)),
                Some(AnyOf(vals)) => extend_left!(self.deps, vals.iter().map(AsRef::as_ref)),
                Some(ExactlyOneOf(vals)) => extend_left!(self.deps, vals.iter().map(AsRef::as_ref)),
                Some(AtMostOneOf(vals)) => extend_left!(self.deps, vals.iter().map(AsRef::as_ref)),
                Some(UseEnabled(_, vals)) => {
                    extend_left!(self.deps, vals.iter().map(AsRef::as_ref))
                }
                Some(UseDisabled(_, vals)) => {
                    extend_left!(self.deps, vals.iter().map(AsRef::as_ref))
                }
                None => return None,
            }
        }
    }
}

impl Restriction<&DepSet<Atom>> for restrict::Restrict {
    fn matches(&self, val: &DepSet<Atom>) -> bool {
        restrict::restrict_match! {
            self, val,
            Self::Atom(r) => val.flatten().any(|v| r.matches(v))
        }
    }
}

peg::parser!(grammar depset() for str {
    rule _ = [' ']

    // Technically PROPERTIES and RESTRICT tokens have no restrictions, but use license
    // restrictions in order to properly parse use restrictions.
    rule properties_restrict_val() -> DepRestrict<String>
        = s:$(quiet!{
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '_']
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '_' | '.' | '-']*
        } / expected!("string value")
        ) { DepRestrict::Matches(s.to_string(), true) }

    // licenses must not begin with a hyphen, dot, or plus sign.
    rule license_val() -> DepRestrict<String>
        = s:$(quiet!{
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '_']
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '_' | '.' | '-']*
        } / expected!("license name")
        ) { DepRestrict::Matches(s.to_string(), true) }

    rule useflag() -> &'input str
        = s:$(quiet!{
            ['a'..='z' | 'A'..='Z' | '0'..='9']
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '_' | '@' | '-']*
        } / expected!("useflag name")
        ) { s }

    rule useflag_val() -> DepRestrict<String>
        = disabled:"!"? s:useflag() {
            DepRestrict::Matches(s.to_string(), disabled.is_none())
        }

    rule pkg_val(eapi: &'static Eapi) -> DepRestrict<Atom>
        = s:$(quiet!{!")" [^' ']+}) {?
            let atom = match Atom::new(s, eapi) {
                Ok(x) => x,
                Err(e) => return Err("failed parsing atom"),
            };
            Ok(DepRestrict::Matches(atom, true))
        }

    rule uri_val(eapi: &'static Eapi) -> DepRestrict<Uri>
        = s:$(quiet!{!")" [^' ']+}) rename:(_ "->" _ s:$([^' ']+) {s})? {?
            let mut uri = Uri { uri: s.to_string(), rename: None };
            if let Some(r) = rename {
                if !eapi.has(Feature::SrcUriRenames) {
                    return Err("SRC_URI renames available in EAPI >= 2");
                }
                uri.rename = Some(r.to_string());
            }
            Ok(DepRestrict::Matches(uri, true))
        }

    rule parens<T>(expr: rule<T>) -> Vec<Box<T>>
        = "(" _ v:expr() ++ " " _ ")"
        { v.into_iter().map(Box::new).collect() }

    rule all_of<T>(expr: rule<DepRestrict<T>>) -> DepRestrict<T>
        = vals:parens(<expr()>) { DepRestrict::AllOf(vals) }

    rule any_of<T>(expr: rule<DepRestrict<T>>) -> DepRestrict<T>
        = "||" _ vals:parens(<expr()>) { DepRestrict::AnyOf(vals) }

    rule use_cond<T>(expr: rule<DepRestrict<T>>) -> DepRestrict<T>
        = negate:"!"? u:useflag() "?" _ vals:parens(<expr()>) {
            let f = match negate {
                None => DepRestrict::UseEnabled,
                Some(_) => DepRestrict::UseDisabled,
            };
            f(u.to_string(), vals)
        }

    rule exactly_one_of<T>(expr: rule<DepRestrict<T>>) -> DepRestrict<T>
        = "^^" _ vals:parens(<expr()>) { DepRestrict::ExactlyOneOf(vals) }

    rule at_most_one_of<T>(eapi: &'static Eapi, expr: rule<DepRestrict<T>>) -> DepRestrict<T>
        = "??" _ vals:parens(<expr()>) {?
            if !eapi.has(Feature::RequiredUseOneOf) {
                return Err("?? groups are supported in >= EAPI 5");
            }
            Ok(DepRestrict::AtMostOneOf(vals))
        }

    rule license_dep_restrict() -> DepRestrict<String>
        = use_cond(<license_dep_restrict()>)
            / any_of(<license_dep_restrict()>)
            / all_of(<license_dep_restrict()>)
            / license_val()

    rule src_uri_dep_restrict(eapi: &'static Eapi) -> DepRestrict<Uri>
        = use_cond(<src_uri_dep_restrict(eapi)>)
            / all_of(<src_uri_dep_restrict(eapi)>)
            / uri_val(eapi)

    rule properties_dep_restrict() -> DepRestrict<String>
        = use_cond(<properties_dep_restrict()>)
            / all_of(<properties_dep_restrict()>)
            / properties_restrict_val()

    rule required_use_dep_restrict(eapi: &'static Eapi) -> DepRestrict<String>
        = use_cond(<required_use_dep_restrict(eapi)>)
            / any_of(<required_use_dep_restrict(eapi)>)
            / all_of(<required_use_dep_restrict(eapi)>)
            / exactly_one_of(<required_use_dep_restrict(eapi)>)
            / at_most_one_of(eapi, <required_use_dep_restrict(eapi)>)
            / useflag_val()

    rule restrict_dep_restrict() -> DepRestrict<String>
        = use_cond(<restrict_dep_restrict()>)
            / all_of(<restrict_dep_restrict()>)
            / properties_restrict_val()

    rule pkg_dep_restrict(eapi: &'static Eapi) -> DepRestrict<Atom>
        = use_cond(<pkg_dep_restrict(eapi)>)
            / any_of(<pkg_dep_restrict(eapi)>)
            / all_of(<pkg_dep_restrict(eapi)>)
            / pkg_val(eapi)

    pub(super) rule license() -> DepSet<String>
        = deps:license_dep_restrict() ++ " " { DepSet { deps } }

    pub(super) rule src_uri(eapi: &'static Eapi) -> DepSet<Uri>
        = deps:src_uri_dep_restrict(eapi) ++ " " { DepSet { deps } }

    pub(super) rule properties() -> DepSet<String>
        = deps:properties_dep_restrict() ++ " " { DepSet { deps } }

    pub(super) rule required_use(eapi: &'static Eapi) -> DepSet<String>
        = deps:required_use_dep_restrict(eapi) ++ " " { DepSet { deps } }

    pub(super) rule restrict() -> DepSet<String>
        = deps:restrict_dep_restrict() ++ " " { DepSet { deps } }

    pub(super) rule pkgdep(eapi: &'static Eapi) -> DepSet<Atom>
        = deps:pkg_dep_restrict(eapi) ++ " " { DepSet { deps } }
});

// provide public parsing functionality while converting error types
pub mod parse {
    use crate::peg::peg_error;

    use super::*;

    pub fn license(s: &str) -> crate::Result<Option<DepSet<String>>> {
        match s.is_empty() {
            true => Ok(None),
            false => depset::license(s)
                .map(Some)
                .map_err(|e| peg_error(format!("invalid LICENSE: {s:?}"), s, e)),
        }
    }

    pub fn src_uri(s: &str, eapi: &'static Eapi) -> crate::Result<Option<DepSet<Uri>>> {
        match s.is_empty() {
            true => Ok(None),
            false => depset::src_uri(s, eapi)
                .map(Some)
                .map_err(|e| peg_error(format!("invalid SRC_URI: {s:?}"), s, e)),
        }
    }

    pub fn properties(s: &str) -> crate::Result<Option<DepSet<String>>> {
        match s.is_empty() {
            true => Ok(None),
            false => depset::properties(s)
                .map(Some)
                .map_err(|e| peg_error(format!("invalid PROPERTIES: {s:?}"), s, e)),
        }
    }

    pub fn required_use(s: &str, eapi: &'static Eapi) -> crate::Result<Option<DepSet<String>>> {
        match s.is_empty() {
            true => Ok(None),
            false => depset::required_use(s, eapi)
                .map(Some)
                .map_err(|e| peg_error(format!("invalid REQUIRED_USE: {s:?}"), s, e)),
        }
    }

    pub fn restrict(s: &str) -> crate::Result<Option<DepSet<String>>> {
        match s.is_empty() {
            true => Ok(None),
            false => depset::restrict(s)
                .map(Some)
                .map_err(|e| peg_error(format!("invalid RESTRICT: {s:?}"), s, e)),
        }
    }

    pub fn pkgdep(s: &str, eapi: &'static Eapi) -> crate::Result<Option<DepSet<Atom>>> {
        match s.is_empty() {
            true => Ok(None),
            false => depset::pkgdep(s, eapi)
                .map(Some)
                .map_err(|e| peg_error(format!("invalid dependency: {s:?}"), s, e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::eapi::{EAPIS, EAPI_LATEST};

    use super::DepRestrict::*;
    use super::*;

    fn vs(val: &str) -> DepRestrict<String> {
        Matches(val.to_string(), true)
    }

    fn vd(val: &str) -> DepRestrict<String> {
        Matches(val.to_string(), false)
    }

    fn va(val: &str) -> DepRestrict<Atom> {
        Matches(Atom::from_str(val).unwrap(), true)
    }

    fn vu(u1: &str, u2: Option<&str>) -> DepRestrict<Uri> {
        let uri = Uri {
            uri: u1.to_string(),
            rename: u2.map(String::from),
        };
        Matches(uri, true)
    }

    fn allof<I, T>(val: I) -> DepRestrict<T>
    where
        I: IntoIterator<Item = DepRestrict<T>>,
    {
        AllOf(val.into_iter().map(Box::new).collect())
    }

    fn anyof<I, T>(val: I) -> DepRestrict<T>
    where
        I: IntoIterator<Item = DepRestrict<T>>,
    {
        AnyOf(val.into_iter().map(Box::new).collect())
    }

    fn exactly_one_of<I, T>(val: I) -> DepRestrict<T>
    where
        I: IntoIterator<Item = DepRestrict<T>>,
    {
        ExactlyOneOf(val.into_iter().map(Box::new).collect())
    }

    fn at_most_one_of<I, T>(val: I) -> DepRestrict<T>
    where
        I: IntoIterator<Item = DepRestrict<T>>,
    {
        AtMostOneOf(val.into_iter().map(Box::new).collect())
    }

    fn use_enabled<I, T>(s: &str, val: I) -> DepRestrict<T>
    where
        I: IntoIterator<Item = DepRestrict<T>>,
    {
        UseEnabled(s.to_string(), val.into_iter().map(Box::new).collect())
    }

    fn use_disabled<I, T>(s: &str, val: I) -> DepRestrict<T>
    where
        I: IntoIterator<Item = DepRestrict<T>>,
    {
        UseDisabled(s.to_string(), val.into_iter().map(Box::new).collect())
    }

    #[test]
    fn test_license() -> crate::Result<()> {
        // invalid
        for s in ["(", ")", "( )", "( l1)", "| ( l1 )", "!use ( l1 )"] {
            assert!(parse::license(&s).is_err(), "{s:?} didn't fail");
        }

        // empty string
        assert!(parse::license("").unwrap().is_none());

        // valid
        for (s, expected, expected_flatten) in [
            // simple values
            ("v", vec![vs("v")], vec!["v"]),
            ("v1 v2", vec![vs("v1"), vs("v2")], vec!["v1", "v2"]),
            // groupings
            ("( v )", vec![allof(vec![vs("v")])], vec!["v"]),
            ("( v1 v2 )", vec![allof(vec![vs("v1"), vs("v2")])], vec!["v1", "v2"]),
            ("( v1 ( v2 ) )", vec![allof(vec![vs("v1"), allof(vec![vs("v2")])])], vec!["v1", "v2"]),
            ("( ( v ) )", vec![allof(vec![allof(vec![vs("v")])])], vec!["v"]),
            ("|| ( v )", vec![anyof(vec![vs("v")])], vec!["v"]),
            ("|| ( v1 v2 )", vec![anyof(vec![vs("v1"), vs("v2")])], vec!["v1", "v2"]),
            // conditionals
            ("u? ( v )", vec![use_enabled("u", vec![vs("v")])], vec!["v"]),
            ("u? ( v1 v2 )", vec![use_enabled("u", [vs("v1"), vs("v2")])], vec!["v1", "v2"]),
            // combinations
            ("v1 u? ( v2 )", vec![vs("v1"), use_enabled("u", [vs("v2")])], vec!["v1", "v2"]),
            (
                "!u? ( || ( v1 v2 ) )",
                vec![use_disabled("u", [anyof([vs("v1"), vs("v2")])])],
                vec!["v1", "v2"],
            ),
        ] {
            let depset = parse::license(&s)?.unwrap();
            let flatten: Vec<_> = depset.flatten().collect();
            assert_eq!(flatten, expected_flatten);
            assert_eq!(depset.deps, expected, "{s} failed");
            assert_eq!(depset.to_string(), s);
        }

        Ok(())
    }

    #[test]
    fn test_src_uri() -> crate::Result<()> {
        // empty string
        assert!(parse::src_uri("", &EAPI_LATEST).unwrap().is_none());

        // valid
        for (s, expected, expected_flatten) in [
            ("uri", vec![vu("uri", None)], vec!["uri"]),
            ("http://uri", vec![vu("http://uri", None)], vec!["http://uri"]),
            ("uri1 uri2", vec![vu("uri1", None), vu("uri2", None)], vec!["uri1", "uri2"]),
            (
                "( http://uri1 http://uri2 )",
                vec![allof([vu("http://uri1", None), vu("http://uri2", None)])],
                vec!["http://uri1", "http://uri2"],
            ),
            (
                "u? ( http://uri1 )",
                vec![use_enabled("u", [vu("http://uri1", None)])],
                vec!["http://uri1"],
            ),
        ] {
            for eapi in EAPIS.values() {
                let depset = parse::src_uri(&s, eapi)?.unwrap();
                let flatten: Vec<_> = depset.flatten().map(|x| x.to_string()).collect();
                assert_eq!(flatten, expected_flatten);
                assert_eq!(depset.deps, expected, "{s} failed");
                assert_eq!(depset.to_string(), s);
            }
        }

        // SRC_URI renames
        for (s, expected, expected_flatten) in [(
            "http://uri -> file",
            vec![vu("http://uri", Some("file"))],
            vec!["http://uri -> file"],
        )] {
            for eapi in EAPIS.values() {
                if eapi.has(Feature::SrcUriRenames) {
                    let depset = parse::src_uri(&s, eapi)?.unwrap();
                    let flatten: Vec<_> = depset.flatten().map(|x| x.to_string()).collect();
                    assert_eq!(flatten, expected_flatten);
                    assert_eq!(depset.deps, expected, "{s} failed");
                    assert_eq!(depset.to_string(), s);
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_required_use() -> crate::Result<()> {
        // invalid
        for s in ["(", ")", "( )", "( u)", "| ( u )"] {
            assert!(parse::required_use(&s, &EAPI_LATEST).is_err(), "{s:?} didn't fail");
        }

        // empty string
        assert!(parse::required_use("", &EAPI_LATEST).unwrap().is_none());

        // valid
        for (s, expected, expected_flatten) in [
            ("u", vec![vs("u")], vec!["u"]),
            ("!u", vec![vd("u")], vec!["u"]),
            ("u1 !u2", vec![vs("u1"), vd("u2")], vec!["u1", "u2"]),
            ("( u )", vec![allof([vs("u")])], vec!["u"]),
            ("( u1 u2 )", vec![allof([vs("u1"), vs("u2")])], vec!["u1", "u2"]),
            ("|| ( u )", vec![anyof([vs("u")])], vec!["u"]),
            ("|| ( !u1 u2 )", vec![anyof([vd("u1"), vs("u2")])], vec!["u1", "u2"]),
            ("^^ ( u1 !u2 )", vec![exactly_one_of([vs("u1"), vd("u2")])], vec!["u1", "u2"]),
            ("u1? ( u2 )", vec![use_enabled("u1", [vs("u2")])], vec!["u2"]),
            ("u1? ( u2 !u3 )", vec![use_enabled("u1", [vs("u2"), vd("u3")])], vec!["u2", "u3"]),
            (
                "!u1? ( || ( u2 u3 ) )",
                vec![use_disabled("u1", [anyof([vs("u2"), vs("u3")])])],
                vec!["u2", "u3"],
            ),
        ] {
            let depset = parse::required_use(&s, &EAPI_LATEST)?.unwrap();
            let flatten: Vec<_> = depset.flatten().collect();
            assert_eq!(flatten, expected_flatten);
            assert_eq!(depset.deps, expected, "{s} failed");
            assert_eq!(depset.to_string(), s);
        }

        // ?? operator
        for (s, expected, expected_flatten) in
            [("?? ( u1 u2 )", vec![at_most_one_of([vs("u1"), vs("u2")])], vec!["u1", "u2"])]
        {
            for eapi in EAPIS.values() {
                if eapi.has(Feature::RequiredUseOneOf) {
                    let depset = parse::required_use(&s, eapi)?.unwrap();
                    let flatten: Vec<_> = depset.flatten().collect();
                    assert_eq!(flatten, expected_flatten);
                    assert_eq!(depset.deps, expected, "{s} failed");
                    assert_eq!(depset.to_string(), s);
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_pkgdep() -> crate::Result<()> {
        // invalid
        for s in ["(", ")", "( )", "( a/b)", "| ( a/b )", "use ( a/b )", "!use ( a/b )"] {
            assert!(parse::pkgdep(&s, &EAPI_LATEST).is_err(), "{s:?} didn't fail");
        }

        // empty string
        assert!(parse::pkgdep("", &EAPI_LATEST).unwrap().is_none());

        // valid
        for (s, expected, expected_flatten) in [
            ("a/b", vec![va("a/b")], vec!["a/b"]),
            ("a/b c/d", vec![va("a/b"), va("c/d")], vec!["a/b", "c/d"]),
            ("( a/b c/d )", vec![allof([va("a/b"), va("c/d")])], vec!["a/b", "c/d"]),
            ("u? ( a/b c/d )", vec![use_enabled("u", [va("a/b"), va("c/d")])], vec!["a/b", "c/d"]),
            (
                "!u? ( a/b c/d )",
                vec![use_disabled("u", [va("a/b"), va("c/d")])],
                vec!["a/b", "c/d"],
            ),
            (
                "u1? ( a/b !u2? ( c/d ) )",
                vec![use_enabled("u1", [va("a/b"), use_disabled("u2", [va("c/d")])])],
                vec!["a/b", "c/d"],
            ),
        ] {
            let depset = parse::pkgdep(&s, &EAPI_LATEST)?.unwrap();
            let flatten: Vec<_> = depset.flatten().map(|x| x.to_string()).collect();
            assert_eq!(flatten, expected_flatten);
            assert_eq!(depset.deps, expected, "{s} failed");
            assert_eq!(depset.to_string(), s);
        }

        Ok(())
    }

    #[test]
    fn test_properties_restrict() -> crate::Result<()> {
        for parse_func in [parse::properties, parse::restrict] {
            // invalid
            for s in ["(", ")", "( )", "( v)", "| ( v )", "!use ( v )", "|| ( v )", "|| ( v1 v2 )"]
            {
                assert!(parse_func(&s).is_err(), "{s:?} didn't fail");
            }

            // empty string
            assert!(parse_func("").unwrap().is_none());

            // valid
            for (s, expected, expected_flatten) in [
                // simple values
                ("v", vec![vs("v")], vec!["v"]),
                ("v1 v2", vec![vs("v1"), vs("v2")], vec!["v1", "v2"]),
                // groupings
                ("( v )", vec![allof(vec![vs("v")])], vec!["v"]),
                ("( v1 v2 )", vec![allof(vec![vs("v1"), vs("v2")])], vec!["v1", "v2"]),
                (
                    "( v1 ( v2 ) )",
                    vec![allof(vec![vs("v1"), allof(vec![vs("v2")])])],
                    vec!["v1", "v2"],
                ),
                ("( ( v ) )", vec![allof(vec![allof(vec![vs("v")])])], vec!["v"]),
                // conditionals
                ("u? ( v )", vec![use_enabled("u", vec![vs("v")])], vec!["v"]),
                ("u? ( v1 v2 )", vec![use_enabled("u", [vs("v1"), vs("v2")])], vec!["v1", "v2"]),
                ("!u? ( v1 v2 )", vec![use_disabled("u", [vs("v1"), vs("v2")])], vec!["v1", "v2"]),
                // combinations
                ("v1 u? ( v2 )", vec![vs("v1"), use_enabled("u", [vs("v2")])], vec!["v1", "v2"]),
            ] {
                let depset = parse_func(&s)?.unwrap();
                let flatten: Vec<_> = depset.flatten().collect();
                assert_eq!(flatten, expected_flatten);
                assert_eq!(depset.deps, expected, "{s} failed");
                assert_eq!(depset.to_string(), s);
            }
        }

        Ok(())
    }
}
