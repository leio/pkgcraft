use std::str::FromStr;

use cached::{proc_macro::cached, SizedCache};

use crate::dep::cpv::ParsedCpv;
use crate::dep::pkg::ParsedDep;
use crate::dep::version::{ParsedVersion, Suffix};
use crate::dep::{Blocker, Cpv, Dep, DepSet, DepSpec, SlotOperator, Uri, Version};
use crate::eapi::{Eapi, Feature};
use crate::peg::peg_error;
use crate::set::Ordered;
use crate::Error;

peg::parser!(grammar depspec() for str {
    // Categories must not begin with a hyphen, dot, or plus sign.
    pub(super) rule category() -> &'input str
        = s:$(quiet!{
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '_']
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '_' | '.' | '-']*
        } / expected!("category name"))
        { s }

    // Packages must not begin with a hyphen or plus sign and must not end in a
    // hyphen followed by anything matching a version.
    pub(super) rule package() -> &'input str
        = s:$(quiet!{
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '_']
            (['a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '_'] /
                ("-" !(version() ("-" version())? ![_])))*
        } / expected!("package name"))
        { s }

    rule version_suffix() -> Suffix
        = "_" suffix:$("alpha" / "beta" / "pre" / "rc" / "p") ver:$(['0'..='9']+)? {?
            let num = ver.map(|s| s.parse().map_err(|_| "version suffix integer overflow"));
            let suffix = match suffix {
                "alpha" => Suffix::Alpha,
                "beta" => Suffix::Beta,
                "pre" => Suffix::Pre,
                "rc" => Suffix::Rc,
                "p" => Suffix::P,
                _ => panic!("invalid suffix"),
            };
            Ok(suffix(num.transpose()?))
        }

    // TODO: figure out how to return string slice instead of positions
    // Related issue: https://github.com/kevinmehall/rust-peg/issues/283
    pub(super) rule version() -> ParsedVersion<'input>
        = start:position!() numbers:$(['0'..='9']+) ++ "." letter:['a'..='z']?
                suffixes:version_suffix()*
                end_base:position!() revision:revision()? end:position!() {
            ParsedVersion {
                start,
                end,
                base_end: end_base-start,
                op: None,
                numbers,
                letter,
                suffixes,
                revision,
            }
        }

    pub(super) rule version_with_op() -> ParsedVersion<'input>
        = op:$(("<" "="?) / "=" / "~" / (">" "="?)) v:version() glob:$("*")? {?
            v.with_op(op, glob)
        }

    rule revision() -> &'input str
        = "-r" s:$(quiet!{['0'..='9']+} / expected!("revision"))
        { s }

    // Slot names must not begin with a hyphen, dot, or plus sign.
    rule slot_name() -> &'input str
        = s:$(quiet!{
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '_']
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '_' | '.' | '-']*
        } / expected!("slot name")
        ) { s }

    rule slot_dep(eapi: &'static Eapi) -> (Option<&'input str>, Option<&'input str>, Option<SlotOperator>)
        = ":" slot_parts:slot_str(eapi) {?
            if eapi.has(Feature::SlotDeps) {
                Ok(slot_parts)
            } else {
                Err("slot deps are supported in >= EAPI 1")
            }
        }

    rule slot_str(eapi: &'static Eapi) -> (Option<&'input str>, Option<&'input str>, Option<SlotOperator>)
        = s:$("*" / "=") {?
            if eapi.has(Feature::SlotOps) {
                let op = SlotOperator::from_str(s).map_err(|_| "invalid slot operator")?;
                Ok((None, None, Some(op)))
            } else {
                Err("slot operators are supported in >= EAPI 5")
            }
        } / slot:slot(eapi) op:$("=")? {?
            match (op.is_some(), eapi.has(Feature::SlotOps)) {
                (true, false) => Err("slot operators are supported in >= EAPI 5"),
                _ => Ok((Some(slot.0), slot.1, op.map(|_| SlotOperator::Equal))),
            }
        }

    rule slot(eapi: &'static Eapi) -> (&'input str, Option<&'input str>)
        = slot:slot_name() subslot:subslot(eapi)? {
            (slot, subslot)
        }

    rule subslot(eapi: &'static Eapi) -> &'input str
        = "/" s:slot_name() {?
            if eapi.has(Feature::Subslots) {
                Ok(s)
            } else {
                Err("subslots are supported in >= EAPI 5")
            }
        }

    rule blocker(eapi: &'static Eapi) -> Blocker
        = s:$("!" "!"?) {?
            if eapi.has(Feature::Blockers) {
                Blocker::from_str(s).map_err(|_| "invalid blocker")
            } else {
                Err("blockers are supported in >= EAPI 2")
            }
        }

    pub(super) rule use_flag() -> &'input str
        = s:$(quiet!{
            ['a'..='z' | 'A'..='Z' | '0'..='9']
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '_' | '@' | '-']*
        } / expected!("USE flag name")
        ) { s }

    rule use_dep(eapi: &'static Eapi) -> &'input str
        = s:$(quiet!{
            (use_flag() use_dep_default(eapi)? ['=' | '?']?) /
            ("-" use_flag() use_dep_default(eapi)?) /
            ("!" use_flag() use_dep_default(eapi)? ['=' | '?'])
        } / expected!("use dep")
        ) { s }

    rule use_deps(eapi: &'static Eapi) -> Vec<&'input str>
        = "[" use_deps:use_dep(eapi) ++ "," "]" {?
            if eapi.has(Feature::UseDeps) {
                Ok(use_deps)
            } else {
                Err("use deps are supported in >= EAPI 2")
            }
        }

    rule use_dep_default(eapi: &'static Eapi) -> &'input str
        = s:$("(+)" / "(-)") {?
            if eapi.has(Feature::UseDepDefaults) {
                Ok(s)
            } else {
                Err("use dep defaults are supported in >= EAPI 4")
            }
        }

    // repo must not begin with a hyphen and must also be a valid package name
    pub(super) rule repo() -> &'input str
        = s:$(quiet!{
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '_']
            (['a'..='z' | 'A'..='Z' | '0'..='9' | '_'] / ("-" !version()))*
        } / expected!("repo name")
        ) { s }

    rule repo_dep(eapi: &'static Eapi) -> &'input str
        = "::" repo:repo() {?
            if eapi.has(Feature::RepoIds) {
                Ok(repo)
            } else {
                Err("repo deps aren't supported in EAPIs")
            }
        }

    pub(super) rule cpv() -> ParsedCpv<'input>
        = category:category() "/" package:package() "-" version:version() {
            ParsedCpv {
                category,
                package,
                version,
                version_str: "",
            }
        }

    pub(super) rule cpv_with_op() -> (&'input str, &'input str, Option<&'input str>)
        = op:$(("<" "="?) / "=" / "~" / (">" "="?)) cpv:$([^'*']+) glob:$("*")?
        { (op, cpv, glob) }

    pub(super) rule cp() -> ParsedDep<'input>
        = category:category() "/" package:package() {
            ParsedDep { category, package, ..Default::default() }
        }

    pub(super) rule dep(eapi: &'static Eapi) -> (&'input str, ParsedDep<'input>)
        = blocker:blocker(eapi)? dep:$([^':' | '[']+) slot_dep:slot_dep(eapi)?
                use_deps:use_deps(eapi)? repo:repo_dep(eapi)? {
            let (slot, subslot, slot_op) = slot_dep.unwrap_or_default();
            (dep, ParsedDep {
                blocker,
                slot,
                subslot,
                slot_op,
                use_deps,
                repo,
                ..Default::default()
            })
        }

    rule _ = [' ']

    // Technically PROPERTIES and RESTRICT tokens have no restrictions, but use license
    // restrictions in order to properly parse use restrictions.
    rule properties_restrict_val() -> DepSpec<String>
        = s:$(quiet!{
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '_']
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '_' | '.' | '-']*
        } / expected!("string value")
        ) { DepSpec::Enabled(s.to_string()) }

    // licenses must not begin with a hyphen, dot, or plus sign.
    rule license_val() -> DepSpec<String>
        = s:$(quiet!{
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '_']
            ['a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '_' | '.' | '-']*
        } / expected!("license name")
        ) { DepSpec::Enabled(s.to_string()) }

    rule use_flag_val() -> DepSpec<String>
        = disabled:"!"? s:use_flag() {
            let val = s.to_string();
            if disabled.is_none() {
                DepSpec::Enabled(val)
            } else {
                DepSpec::Disabled(val)
            }
        }

    rule pkg_val(eapi: &'static Eapi) -> DepSpec<Dep>
        = s:$(quiet!{!")" [^' ']+}) {?
            let dep = match Dep::new(s, eapi) {
                Ok(x) => x,
                Err(e) => return Err("failed parsing dep"),
            };
            Ok(DepSpec::Enabled(dep))
        }

    rule uri_val(eapi: &'static Eapi) -> DepSpec<Uri>
        = s:$(quiet!{!")" [^' ']+}) rename:(_ "->" _ s:$([^' ']+) {s})? {?
            if rename.is_some() && !eapi.has(Feature::SrcUriRenames) {
                return Err("SRC_URI renames available in EAPI >= 2");
            }
            let uri = Uri::new(s, rename).map_err(|_| "invalid URI")?;
            Ok(DepSpec::Enabled(uri))
        }

    rule parens<T: Ordered>(expr: rule<T>) -> Vec<T>
        = "(" _ v:expr() ++ " " _ ")" { v }

    rule all_of<T: Ordered>(expr: rule<DepSpec<T>>) -> DepSpec<T>
        = vals:parens(<expr()>)
        { DepSpec::AllOf(vals.into_iter().map(Box::new).collect()) }

    rule any_of<T: Ordered>(expr: rule<DepSpec<T>>) -> DepSpec<T>
        = "||" _ vals:parens(<expr()>)
        { DepSpec::AnyOf(vals.into_iter().map(Box::new).collect()) }

    rule use_cond<T: Ordered>(expr: rule<DepSpec<T>>) -> DepSpec<T>
        = negate:"!"? u:use_flag() "?" _ vals:parens(<expr()>) {
            let f = match negate {
                None => DepSpec::UseEnabled,
                Some(_) => DepSpec::UseDisabled,
            };
            f(u.to_string(), vals.into_iter().map(Box::new).collect())
        }

    rule exactly_one_of<T: Ordered>(expr: rule<DepSpec<T>>) -> DepSpec<T>
        = "^^" _ vals:parens(<expr()>)
        { DepSpec::ExactlyOneOf(vals.into_iter().map(Box::new).collect()) }

    rule at_most_one_of<T: Ordered>(eapi: &'static Eapi, expr: rule<DepSpec<T>>) -> DepSpec<T>
        = "??" _ vals:parens(<expr()>) {?
            if !eapi.has(Feature::RequiredUseOneOf) {
                return Err("?? groups are supported in >= EAPI 5");
            }
            Ok(DepSpec::AtMostOneOf(vals.into_iter().map(Box::new).collect()))
        }

    rule license_dep_restrict() -> DepSpec<String>
        = use_cond(<license_dep_restrict()>)
            / any_of(<license_dep_restrict()>)
            / all_of(<license_dep_restrict()>)
            / license_val()

    rule src_uri_dep_restrict(eapi: &'static Eapi) -> DepSpec<Uri>
        = use_cond(<src_uri_dep_restrict(eapi)>)
            / all_of(<src_uri_dep_restrict(eapi)>)
            / uri_val(eapi)

    rule properties_dep_restrict() -> DepSpec<String>
        = use_cond(<properties_dep_restrict()>)
            / all_of(<properties_dep_restrict()>)
            / properties_restrict_val()

    rule required_use_dep_restrict(eapi: &'static Eapi) -> DepSpec<String>
        = use_cond(<required_use_dep_restrict(eapi)>)
            / any_of(<required_use_dep_restrict(eapi)>)
            / all_of(<required_use_dep_restrict(eapi)>)
            / exactly_one_of(<required_use_dep_restrict(eapi)>)
            / at_most_one_of(eapi, <required_use_dep_restrict(eapi)>)
            / use_flag_val()

    rule restrict_dep_restrict() -> DepSpec<String>
        = use_cond(<restrict_dep_restrict()>)
            / all_of(<restrict_dep_restrict()>)
            / properties_restrict_val()

    rule dependencies_restrict(eapi: &'static Eapi) -> DepSpec<Dep>
        = use_cond(<dependencies_restrict(eapi)>)
            / any_of(<dependencies_restrict(eapi)>)
            / all_of(<dependencies_restrict(eapi)>)
            / pkg_val(eapi)

    pub(super) rule license() -> DepSet<String>
        = v:license_dep_restrict() ++ " " { DepSet::from_iter(v) }

    pub(super) rule src_uri(eapi: &'static Eapi) -> DepSet<Uri>
        = v:src_uri_dep_restrict(eapi) ++ " " { DepSet::from_iter(v) }

    pub(super) rule properties() -> DepSet<String>
        = v:properties_dep_restrict() ++ " " { DepSet::from_iter(v) }

    pub(super) rule required_use(eapi: &'static Eapi) -> DepSet<String>
        = v:required_use_dep_restrict(eapi) ++ " " { DepSet::from_iter(v) }

    pub(super) rule restrict() -> DepSet<String>
        = v:restrict_dep_restrict() ++ " " { DepSet::from_iter(v) }

    pub(super) rule dependencies(eapi: &'static Eapi) -> DepSet<Dep>
        = v:dependencies_restrict(eapi) ++ " " { DepSet::from_iter(v) }
});

pub fn category(s: &str) -> crate::Result<&str> {
    depspec::category(s).map_err(|e| peg_error(format!("invalid category name: {s}"), s, e))?;
    Ok(s)
}

pub fn package(s: &str) -> crate::Result<&str> {
    depspec::package(s).map_err(|e| peg_error(format!("invalid package name: {s}"), s, e))?;
    Ok(s)
}

pub(super) fn version_str(s: &str) -> crate::Result<ParsedVersion> {
    depspec::version(s).map_err(|e| peg_error(format!("invalid version: {s}"), s, e))
}

#[cached(
    type = "SizedCache<String, crate::Result<Version>>",
    create = "{ SizedCache::with_size(1000) }",
    convert = r#"{ s.to_string() }"#
)]
pub(super) fn version(s: &str) -> crate::Result<Version> {
    version_str(s)?.into_owned(s)
}

pub(super) fn version_with_op(s: &str) -> crate::Result<Version> {
    let ver = depspec::version_with_op(s)
        .map_err(|e| peg_error(format!("invalid version: {s}"), s, e))?;
    ver.into_owned(s)
}

pub fn use_flag(s: &str) -> crate::Result<&str> {
    depspec::use_flag(s).map_err(|e| peg_error(format!("invalid USE flag name: {s}"), s, e))?;
    Ok(s)
}

pub fn repo(s: &str) -> crate::Result<&str> {
    depspec::repo(s).map_err(|e| peg_error(format!("invalid repo name: {s}"), s, e))?;
    Ok(s)
}

pub(super) fn cpv_str(s: &str) -> crate::Result<ParsedCpv> {
    depspec::cpv(s).map_err(|e| peg_error(format!("invalid cpv: {s}"), s, e))
}

#[cached(
    type = "SizedCache<String, crate::Result<Cpv>>",
    create = "{ SizedCache::with_size(1000) }",
    convert = r#"{ s.to_string() }"#
)]
pub(super) fn cpv(s: &str) -> crate::Result<Cpv> {
    let mut cpv = cpv_str(s)?;
    cpv.version_str = s;
    cpv.into_owned()
}

pub(super) fn dep_str<'a>(s: &'a str, eapi: &'static Eapi) -> crate::Result<ParsedDep<'a>> {
    let (dep_s, mut dep) =
        depspec::dep(s, eapi).map_err(|e| peg_error(format!("invalid dep: {s}"), s, e))?;
    match depspec::cpv_with_op(dep_s) {
        Ok((op, cpv_s, glob)) => {
            let cpv = depspec::cpv(cpv_s)
                .map_err(|e| peg_error(format!("invalid dep: {s}"), cpv_s, e))?;
            dep.category = cpv.category;
            dep.package = cpv.package;
            dep.version = Some(
                cpv.version
                    .with_op(op, glob)
                    .map_err(|e| Error::InvalidValue(format!("invalid dep: {s}: {e}")))?,
            );
            dep.version_str = Some(cpv_s);
        }
        _ => {
            let d =
                depspec::cp(dep_s).map_err(|e| peg_error(format!("invalid dep: {s}"), dep_s, e))?;
            dep.category = d.category;
            dep.package = d.package;
        }
    }

    Ok(dep)
}

#[cached(
    type = "SizedCache<(String, &Eapi), crate::Result<Dep>>",
    create = "{ SizedCache::with_size(1000) }",
    convert = r#"{ (s.to_string(), eapi) }"#
)]
pub(super) fn dep(s: &str, eapi: &'static Eapi) -> crate::Result<Dep> {
    let dep = dep_str(s, eapi)?;
    dep.into_owned()
}

pub(super) fn dep_unversioned(s: &str) -> crate::Result<Dep> {
    let dep =
        depspec::cp(s).map_err(|e| peg_error(format!("invalid unversioned dep: {s}"), s, e))?;
    dep.into_owned()
}

pub fn license(s: &str) -> crate::Result<Option<DepSet<String>>> {
    if s.is_empty() {
        Ok(None)
    } else {
        depspec::license(s)
            .map(Some)
            .map_err(|e| peg_error(format!("invalid LICENSE: {s:?}"), s, e))
    }
}

pub fn src_uri(s: &str, eapi: &'static Eapi) -> crate::Result<Option<DepSet<Uri>>> {
    if s.is_empty() {
        Ok(None)
    } else {
        depspec::src_uri(s, eapi)
            .map(Some)
            .map_err(|e| peg_error(format!("invalid SRC_URI: {s:?}"), s, e))
    }
}

pub fn properties(s: &str) -> crate::Result<Option<DepSet<String>>> {
    if s.is_empty() {
        Ok(None)
    } else {
        depspec::properties(s)
            .map(Some)
            .map_err(|e| peg_error(format!("invalid PROPERTIES: {s:?}"), s, e))
    }
}

pub fn required_use(s: &str, eapi: &'static Eapi) -> crate::Result<Option<DepSet<String>>> {
    if s.is_empty() {
        Ok(None)
    } else {
        depspec::required_use(s, eapi)
            .map(Some)
            .map_err(|e| peg_error(format!("invalid REQUIRED_USE: {s:?}"), s, e))
    }
}

pub fn restrict(s: &str) -> crate::Result<Option<DepSet<String>>> {
    if s.is_empty() {
        Ok(None)
    } else {
        depspec::restrict(s)
            .map(Some)
            .map_err(|e| peg_error(format!("invalid RESTRICT: {s:?}"), s, e))
    }
}

pub fn dependencies(s: &str, eapi: &'static Eapi) -> crate::Result<Option<DepSet<Dep>>> {
    if s.is_empty() {
        Ok(None)
    } else {
        depspec::dependencies(s, eapi)
            .map(Some)
            .map_err(|e| peg_error(format!("invalid dependency: {s:?}"), s, e))
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexSet;

    use crate::eapi::{self, EAPIS, EAPIS_OFFICIAL, EAPI_LATEST_OFFICIAL};
    use crate::test::TEST_DATA;

    use super::*;

    #[test]
    fn test_parse() {
        // invalid deps
        for s in &TEST_DATA.dep_toml.invalid {
            for eapi in EAPIS.iter() {
                let result = dep(s, eapi);
                assert!(result.is_err(), "{s:?} didn't fail for EAPI={eapi}");
            }
        }

        // valid deps
        for e in &TEST_DATA.dep_toml.valid {
            let s = e.dep.as_str();
            let passing_eapis: IndexSet<_> = eapi::range(&e.eapis).unwrap().collect();
            // verify parse successes
            for eapi in &passing_eapis {
                let result = dep(s, eapi);
                assert!(result.is_ok(), "{s:?} failed for EAPI={eapi}");
                let d = result.unwrap();
                assert_eq!(d.category(), e.category, "{s:?} failed for EAPI={eapi}");
                assert_eq!(d.package(), e.package, "{s:?} failed for EAPI={eapi}");
                assert_eq!(d.blocker(), e.blocker, "{s:?} failed for EAPI={eapi}");
                assert_eq!(d.version(), e.version.as_ref(), "{s:?} failed for EAPI={eapi}");
                assert_eq!(d.revision(), e.revision.as_ref(), "{s:?} failed for EAPI={eapi}");
                assert_eq!(d.slot(), e.slot.as_deref(), "{s:?} failed for EAPI={eapi}");
                assert_eq!(d.subslot(), e.subslot.as_deref(), "{s:?} failed for EAPI={eapi}");
                assert_eq!(d.slot_op(), e.slot_op, "{s:?} failed for EAPI={eapi}");
                assert_eq!(d.use_deps(), e.use_deps.as_ref(), "{s:?} failed for EAPI={eapi}");
                assert_eq!(d.to_string(), s, "{s:?} failed for EAPI={eapi}");
            }
            // verify parse failures
            for eapi in EAPIS.difference(&passing_eapis) {
                let result = dep(s, eapi);
                assert!(result.is_err(), "{s:?} didn't fail for EAPI={eapi}");
            }
        }
    }

    #[test]
    fn test_parse_slots() {
        // good deps
        for slot in ["0", "a", "_", "_a", "99", "aBc", "a+b_c.d-e"] {
            for eapi in EAPIS.iter() {
                let s = format!("cat/pkg:{slot}");
                let result = dep(&s, eapi);
                if eapi.has(Feature::SlotDeps) {
                    assert!(result.is_ok(), "{s:?} failed: {}", result.err().unwrap());
                    let d = result.unwrap();
                    assert_eq!(d.slot(), Some(slot));
                    assert_eq!(d.to_string(), s);
                } else {
                    assert!(result.is_err(), "{s:?} didn't fail");
                }
            }
        }
    }

    #[test]
    fn test_parse_blockers() {
        // non-blocker
        let d = dep("cat/pkg", &eapi::EAPI2).unwrap();
        assert!(d.blocker().is_none());

        // good deps
        for (s, blocker) in [
            ("!cat/pkg", Some(Blocker::Weak)),
            ("!cat/pkg:0", Some(Blocker::Weak)),
            ("!!cat/pkg", Some(Blocker::Strong)),
            ("!!<cat/pkg-1", Some(Blocker::Strong)),
        ] {
            for eapi in EAPIS.iter() {
                let result = dep(s, eapi);
                if eapi.has(Feature::Blockers) {
                    assert!(
                        result.is_ok(),
                        "{s:?} failed for EAPI {eapi}: {}",
                        result.err().unwrap()
                    );
                    let d = result.unwrap();
                    assert_eq!(d.blocker(), blocker);
                    assert_eq!(d.to_string(), s);
                } else {
                    assert!(result.is_err(), "{s:?} didn't fail");
                }
            }
        }
    }

    #[test]
    fn test_parse_use_deps() {
        // good deps
        for use_deps in ["a", "!a?", "a,b", "-a,-b", "a?,b?", "a,b=,!c=,d?,!e?,-f"] {
            for eapi in EAPIS.iter() {
                let s = format!("cat/pkg[{use_deps}]");
                let result = dep(&s, eapi);
                if eapi.has(Feature::UseDeps) {
                    assert!(result.is_ok(), "{s:?} failed: {}", result.err().unwrap());
                    let d = result.unwrap();
                    let expected = use_deps.split(',').map(|s| s.to_string()).collect();
                    assert_eq!(d.use_deps(), Some(&expected));
                    assert_eq!(d.to_string(), s);
                } else {
                    assert!(result.is_err(), "{s:?} didn't fail");
                }
            }
        }
    }

    #[test]
    fn test_parse_use_dep_defaults() {
        // good deps
        for use_deps in ["a(+)", "-a(-)", "a(+)?,!b(-)?", "a(-)=,!b(+)="] {
            for eapi in EAPIS.iter() {
                let s = format!("cat/pkg[{use_deps}]");
                let result = dep(&s, eapi);
                if eapi.has(Feature::UseDepDefaults) {
                    assert!(result.is_ok(), "{s:?} failed: {}", result.err().unwrap());
                    let d = result.unwrap();
                    let expected = use_deps.split(',').map(|s| s.to_string()).collect();
                    assert_eq!(d.use_deps(), Some(&expected));
                    assert_eq!(d.to_string(), s);
                } else {
                    assert!(result.is_err(), "{s:?} didn't fail");
                }
            }
        }
    }

    #[test]
    fn test_parse_subslots() {
        // good deps
        for (slot_str, slot, subslot, slot_op) in [
            ("0/1", Some("0"), Some("1"), None),
            ("a/b", Some("a"), Some("b"), None),
            ("A/B", Some("A"), Some("B"), None),
            ("_/_", Some("_"), Some("_"), None),
            ("0/a.b+c-d_e", Some("0"), Some("a.b+c-d_e"), None),
        ] {
            for eapi in EAPIS.iter() {
                let s = format!("cat/pkg:{slot_str}");
                let result = dep(&s, eapi);
                if eapi.has(Feature::SlotOps) {
                    assert!(result.is_ok(), "{s:?} failed: {}", result.err().unwrap());
                    let d = result.unwrap();
                    assert_eq!(d.slot(), slot);
                    assert_eq!(d.subslot(), subslot);
                    assert_eq!(d.slot_op(), slot_op);
                    assert_eq!(d.to_string(), s);
                } else {
                    assert!(result.is_err(), "{s:?} didn't fail");
                }
            }
        }
    }

    #[test]
    fn test_parse_slot_ops() {
        // good deps
        for (slot_str, slot, subslot, slot_op) in [
            ("*", None, None, Some(SlotOperator::Star)),
            ("=", None, None, Some(SlotOperator::Equal)),
            ("0=", Some("0"), None, Some(SlotOperator::Equal)),
            ("a=", Some("a"), None, Some(SlotOperator::Equal)),
            ("0/1=", Some("0"), Some("1"), Some(SlotOperator::Equal)),
            ("a/b=", Some("a"), Some("b"), Some(SlotOperator::Equal)),
        ] {
            for eapi in EAPIS.iter() {
                let s = format!("cat/pkg:{slot_str}");
                let result = dep(&s, eapi);
                if eapi.has(Feature::SlotOps) {
                    assert!(result.is_ok(), "{s:?} failed: {}", result.err().unwrap());
                    let d = result.unwrap();
                    assert_eq!(d.slot(), slot);
                    assert_eq!(d.subslot(), subslot);
                    assert_eq!(d.slot_op(), slot_op);
                    assert_eq!(d.to_string(), s);
                } else {
                    assert!(result.is_err(), "{s:?} didn't fail");
                }
            }
        }
    }

    #[test]
    fn test_parse_repos() {
        // repo deps
        for repo in ["_", "a", "repo", "repo_a", "repo-a"] {
            let s = format!("cat/pkg::{repo}");

            // repo ids aren't supported in official EAPIs
            for eapi in EAPIS_OFFICIAL.iter() {
                assert!(dep(&s, eapi).is_err(), "{s:?} didn't fail");
            }

            let result = dep(&s, &eapi::EAPI_PKGCRAFT);
            assert!(result.is_ok(), "{s:?} failed: {}", result.err().unwrap());
            let d = result.unwrap();
            assert_eq!(d.repo(), Some(repo));
            assert_eq!(d.to_string(), s);
        }
    }

    fn vs(val: &str) -> DepSpec<String> {
        DepSpec::Enabled(val.to_string())
    }

    fn vd(val: &str) -> DepSpec<String> {
        DepSpec::Disabled(val.to_string())
    }

    fn vp(val: &str) -> DepSpec<Dep> {
        DepSpec::Enabled(Dep::from_str(val).unwrap())
    }

    fn vu(u1: &str, u2: Option<&str>) -> DepSpec<Uri> {
        DepSpec::Enabled(Uri::new(u1, u2).unwrap())
    }

    fn allof<I, T>(val: I) -> DepSpec<T>
    where
        I: IntoIterator<Item = DepSpec<T>>,
        T: Ordered,
    {
        DepSpec::AllOf(val.into_iter().map(Box::new).collect())
    }

    fn anyof<I, T>(val: I) -> DepSpec<T>
    where
        I: IntoIterator<Item = DepSpec<T>>,
        T: Ordered,
    {
        DepSpec::AnyOf(val.into_iter().map(Box::new).collect())
    }

    fn exactly_one_of<I, T>(val: I) -> DepSpec<T>
    where
        I: IntoIterator<Item = DepSpec<T>>,
        T: Ordered,
    {
        DepSpec::ExactlyOneOf(val.into_iter().map(Box::new).collect())
    }

    fn at_most_one_of<I, T>(val: I) -> DepSpec<T>
    where
        I: IntoIterator<Item = DepSpec<T>>,
        T: Ordered,
    {
        DepSpec::AtMostOneOf(val.into_iter().map(Box::new).collect())
    }

    fn use_enabled<I, T>(s: &str, val: I) -> DepSpec<T>
    where
        I: IntoIterator<Item = DepSpec<T>>,
        T: Ordered,
    {
        DepSpec::UseEnabled(s.to_string(), val.into_iter().map(Box::new).collect())
    }

    fn use_disabled<I, T>(s: &str, val: I) -> DepSpec<T>
    where
        I: IntoIterator<Item = DepSpec<T>>,
        T: Ordered,
    {
        DepSpec::UseDisabled(s.to_string(), val.into_iter().map(Box::new).collect())
    }

    fn ds<I, T>(val: I) -> DepSet<T>
    where
        I: IntoIterator<Item = DepSpec<T>>,
        T: Ordered,
    {
        DepSet::from_iter(val)
    }

    #[test]
    fn test_license() -> crate::Result<()> {
        // invalid
        for s in ["(", ")", "( )", "( l1)", "| ( l1 )", "!use ( l1 )"] {
            assert!(license(s).is_err(), "{s:?} didn't fail");
        }

        // empty string
        assert!(license("").unwrap().is_none());

        // valid
        for (s, expected, expected_flatten) in [
            // simple values
            ("v", ds([vs("v")]), vec!["v"]),
            ("v1 v2", ds([vs("v1"), vs("v2")]), vec!["v1", "v2"]),
            // groupings
            ("( v )", ds([allof(vec![vs("v")])]), vec!["v"]),
            ("( v1 v2 )", ds([allof(vec![vs("v1"), vs("v2")])]), vec!["v1", "v2"]),
            ("( v1 ( v2 ) )", ds([allof(vec![vs("v1"), allof(vec![vs("v2")])])]), vec!["v1", "v2"]),
            ("( ( v ) )", ds([allof(vec![allof(vec![vs("v")])])]), vec!["v"]),
            ("|| ( v )", ds([anyof(vec![vs("v")])]), vec!["v"]),
            ("|| ( v1 v2 )", ds([anyof(vec![vs("v1"), vs("v2")])]), vec!["v1", "v2"]),
            // conditionals
            ("u? ( v )", ds([use_enabled("u", vec![vs("v")])]), vec!["v"]),
            ("u? ( v1 v2 )", ds([use_enabled("u", [vs("v1"), vs("v2")])]), vec!["v1", "v2"]),
            // combinations
            ("v1 u? ( v2 )", ds([vs("v1"), use_enabled("u", [vs("v2")])]), vec!["v1", "v2"]),
            (
                "!u? ( || ( v1 v2 ) )",
                ds([use_disabled("u", [anyof([vs("v1"), vs("v2")])])]),
                vec!["v1", "v2"],
            ),
        ] {
            let depset = license(s)?.unwrap();
            let flatten: Vec<_> = depset.iter_flatten().collect();
            assert_eq!(flatten, expected_flatten);
            assert_eq!(depset, expected, "{s} failed");
            assert_eq!(depset.to_string(), s);
        }

        Ok(())
    }

    #[test]
    fn test_src_uri() -> crate::Result<()> {
        // empty string
        assert!(src_uri("", &EAPI_LATEST_OFFICIAL).unwrap().is_none());

        // valid
        for (s, expected, expected_flatten) in [
            ("uri", ds([vu("uri", None)]), vec!["uri"]),
            ("http://uri", ds([vu("http://uri", None)]), vec!["http://uri"]),
            ("uri1 uri2", ds([vu("uri1", None), vu("uri2", None)]), vec!["uri1", "uri2"]),
            (
                "( http://uri1 http://uri2 )",
                ds([allof([vu("http://uri1", None), vu("http://uri2", None)])]),
                vec!["http://uri1", "http://uri2"],
            ),
            (
                "u1? ( http://uri1 !u2? ( http://uri2 ) )",
                ds([use_enabled(
                    "u1",
                    [vu("http://uri1", None), use_disabled("u2", [vu("http://uri2", None)])],
                )]),
                vec!["http://uri1", "http://uri2"],
            ),
        ] {
            for eapi in EAPIS.iter() {
                let depset = src_uri(s, eapi)?.unwrap();
                let flatten: Vec<_> = depset.iter_flatten().map(|x| x.to_string()).collect();
                assert_eq!(flatten, expected_flatten);
                assert_eq!(depset, expected, "{s} failed");
                assert_eq!(depset.to_string(), s);
            }
        }

        // SRC_URI renames
        for (s, expected, expected_flatten) in [
            (
                "http://uri -> file",
                ds([vu("http://uri", Some("file"))]),
                vec!["http://uri -> file"],
            ),
            (
                "u? ( http://uri -> file )",
                ds([use_enabled("u", [vu("http://uri", Some("file"))])]),
                vec!["http://uri -> file"],
            ),
        ] {
            for eapi in EAPIS.iter() {
                if eapi.has(Feature::SrcUriRenames) {
                    let depset = src_uri(s, eapi)?.unwrap();
                    let flatten: Vec<_> = depset.iter_flatten().map(|x| x.to_string()).collect();
                    assert_eq!(flatten, expected_flatten);
                    assert_eq!(depset, expected, "{s} failed");
                    assert_eq!(depset.to_string(), s);
                }
            }
        }

        for s in ["https://", "https://web/site/root.com/"] {
            let r = src_uri(s, &EAPI_LATEST_OFFICIAL);
            assert!(r.is_err(), "{s:?} didn't fail");
        }

        Ok(())
    }

    #[test]
    fn test_required_use() -> crate::Result<()> {
        // invalid
        for s in ["(", ")", "( )", "( u)", "| ( u )"] {
            assert!(required_use(s, &EAPI_LATEST_OFFICIAL).is_err(), "{s:?} didn't fail");
        }

        // empty string
        assert!(required_use("", &EAPI_LATEST_OFFICIAL).unwrap().is_none());

        // valid
        for (s, expected, expected_flatten) in [
            ("u", ds([vs("u")]), vec!["u"]),
            ("!u", ds([vd("u")]), vec!["u"]),
            ("u1 !u2", ds([vs("u1"), vd("u2")]), vec!["u1", "u2"]),
            ("( u )", ds([allof([vs("u")])]), vec!["u"]),
            ("( u1 u2 )", ds([allof([vs("u1"), vs("u2")])]), vec!["u1", "u2"]),
            ("|| ( u )", ds([anyof([vs("u")])]), vec!["u"]),
            ("|| ( !u1 u2 )", ds([anyof([vd("u1"), vs("u2")])]), vec!["u1", "u2"]),
            ("^^ ( u1 !u2 )", ds([exactly_one_of([vs("u1"), vd("u2")])]), vec!["u1", "u2"]),
            ("u1? ( u2 )", ds([use_enabled("u1", [vs("u2")])]), vec!["u2"]),
            ("u1? ( u2 !u3 )", ds([use_enabled("u1", [vs("u2"), vd("u3")])]), vec!["u2", "u3"]),
            (
                "!u1? ( || ( u2 u3 ) )",
                ds([use_disabled("u1", [anyof([vs("u2"), vs("u3")])])]),
                vec!["u2", "u3"],
            ),
        ] {
            let depset = required_use(s, &EAPI_LATEST_OFFICIAL)?.unwrap();
            let flatten: Vec<_> = depset.iter_flatten().collect();
            assert_eq!(flatten, expected_flatten);
            assert_eq!(depset, expected, "{s} failed");
            assert_eq!(depset.to_string(), s);
        }

        // ?? operator
        for (s, expected, expected_flatten) in
            [("?? ( u1 u2 )", ds([at_most_one_of([vs("u1"), vs("u2")])]), vec!["u1", "u2"])]
        {
            for eapi in EAPIS.iter() {
                if eapi.has(Feature::RequiredUseOneOf) {
                    let depset = required_use(s, eapi)?.unwrap();
                    let flatten: Vec<_> = depset.iter_flatten().collect();
                    assert_eq!(flatten, expected_flatten);
                    assert_eq!(depset, expected, "{s} failed");
                    assert_eq!(depset.to_string(), s);
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_dependencies() -> crate::Result<()> {
        // invalid
        for s in ["(", ")", "( )", "( a/b)", "| ( a/b )", "use ( a/b )", "!use ( a/b )"] {
            assert!(dependencies(s, &EAPI_LATEST_OFFICIAL).is_err(), "{s:?} didn't fail");
        }

        // empty string
        assert!(dependencies("", &EAPI_LATEST_OFFICIAL).unwrap().is_none());

        // valid
        for (s, expected, expected_flatten) in [
            ("a/b", ds([vp("a/b")]), vec!["a/b"]),
            ("a/b c/d", ds([vp("a/b"), vp("c/d")]), vec!["a/b", "c/d"]),
            ("( a/b c/d )", ds([allof([vp("a/b"), vp("c/d")])]), vec!["a/b", "c/d"]),
            ("u? ( a/b c/d )", ds([use_enabled("u", [vp("a/b"), vp("c/d")])]), vec!["a/b", "c/d"]),
            (
                "!u? ( a/b c/d )",
                ds([use_disabled("u", [vp("a/b"), vp("c/d")])]),
                vec!["a/b", "c/d"],
            ),
            (
                "u1? ( a/b !u2? ( c/d ) )",
                ds([use_enabled("u1", [vp("a/b"), use_disabled("u2", [vp("c/d")])])]),
                vec!["a/b", "c/d"],
            ),
        ] {
            let depset = dependencies(s, &EAPI_LATEST_OFFICIAL)?.unwrap();
            let flatten: Vec<_> = depset.iter_flatten().map(|x| x.to_string()).collect();
            assert_eq!(flatten, expected_flatten);
            assert_eq!(depset, expected, "{s} failed");
            assert_eq!(depset.to_string(), s);
        }

        Ok(())
    }

    #[test]
    fn test_properties_restrict() -> crate::Result<()> {
        for parse_func in [properties, restrict] {
            // invalid
            for s in ["(", ")", "( )", "( v)", "| ( v )", "!use ( v )", "|| ( v )", "|| ( v1 v2 )"]
            {
                assert!(parse_func(s).is_err(), "{s:?} didn't fail");
            }

            // empty string
            assert!(parse_func("").unwrap().is_none());

            // valid
            for (s, expected, expected_flatten) in [
                // simple values
                ("v", ds([vs("v")]), vec!["v"]),
                ("v1 v2", ds([vs("v1"), vs("v2")]), vec!["v1", "v2"]),
                // groupings
                ("( v )", ds([allof(vec![vs("v")])]), vec!["v"]),
                ("( v1 v2 )", ds([allof(vec![vs("v1"), vs("v2")])]), vec!["v1", "v2"]),
                (
                    "( v1 ( v2 ) )",
                    ds([allof(vec![vs("v1"), allof(vec![vs("v2")])])]),
                    vec!["v1", "v2"],
                ),
                ("( ( v ) )", ds([allof(vec![allof(vec![vs("v")])])]), vec!["v"]),
                // conditionals
                ("u? ( v )", ds([use_enabled("u", vec![vs("v")])]), vec!["v"]),
                ("u? ( v1 v2 )", ds([use_enabled("u", [vs("v1"), vs("v2")])]), vec!["v1", "v2"]),
                ("!u? ( v1 v2 )", ds([use_disabled("u", [vs("v1"), vs("v2")])]), vec!["v1", "v2"]),
                // combinations
                ("v1 u? ( v2 )", ds([vs("v1"), use_enabled("u", [vs("v2")])]), vec!["v1", "v2"]),
            ] {
                let depset = parse_func(s)?.unwrap();
                let flatten: Vec<_> = depset.iter_flatten().collect();
                assert_eq!(flatten, expected_flatten);
                assert_eq!(depset, expected, "{s} failed");
                assert_eq!(depset.to_string(), s);
            }
        }

        Ok(())
    }
}
