use regex::Regex;

use crate::peg::peg_error;
use crate::pkg::ebuild::Restrict::*;
use crate::restrict::{Restrict, Str};

peg::parser! {
    grammar restrict() for str {
        rule quoted_string() -> &'input str
            = "\"" s:$([^ '\"']+) "\"" { s }
            / "\'" s:$([^ '\'']+) "\'" { s }

        rule string_ops() -> &'input str
            = " "* op:$("==" / "!=" / "=~" / "!~") " "* { op }

        rule str_restrict() -> Restrict
            = attr:$(['a'..='z' | '_']+) op:string_ops() s:quoted_string()
            {?
                let restrict_fn = match attr {
                    "ebuild" => Ebuild,
                    "category" => Category,
                    "description" => Description,
                    "slot" => Slot,
                    "subslot" => Subslot,
                    _ => return Err("unknown package attribute"),
                };

                let r: Restrict = match op {
                    "==" => restrict_fn(Str::matches(s)).into(),
                    "!=" => Restrict::not(restrict_fn(Str::matches(s))),
                    "=~" => match Regex::new(s) {
                        Ok(r) => restrict_fn(Str::Regex(r)).into(),
                        Err(_) => return Err("invalid regex"),
                    },
                    "!~" => match Regex::new(s) {
                        Ok(r) => Restrict::not(restrict_fn(Str::Regex(r))),
                        Err(_) => return Err("invalid regex"),
                    },
                    _ => return Err("invalid string operator"),
                };

                Ok(r)
            }

        rule attr_optional() -> Restrict
            = attr:$(['a'..='z' | '_']+) " is " ("None" / "none") {?
                let r = match attr {
                    "raw_subslot" => RawSubslot(None).into(),
                    "homepage" => Homepage(None).into(),
                    "defined_phases" => DefinedPhases(None).into(),
                    "keywords" => Keywords(None).into(),
                    "iuse" => Iuse(None).into(),
                    "inherit" => Inherit(None).into(),
                    "inherited" => Inherited(None).into(),
                    "long_description" => LongDescription(None).into(),
                    "maintainers" => Maintainers(None).into(),
                    "upstreams" => Upstreams(None).into(),
                    _ => return Err("unknown optional package attribute"),
                };
                Ok(r)
            }

        rule str_restrict_optional() -> Restrict
            = attr:$(['a'..='z' | '_']+) op:string_ops() s:quoted_string() {?
                let restrict_fn = match attr {
                    "raw_subslot" => RawSubslot,
                    "long_description" => LongDescription,
                    _ => return Err("unknown optional package attribute"),
                };

                let r: Restrict = match op {
                    "==" => restrict_fn(Some(Str::matches(s))).into(),
                    "!=" => Restrict::not(restrict_fn(Some(Str::matches(s)))),
                    "=~" => match Regex::new(s) {
                        Ok(r) => restrict_fn(Some(Str::Regex(r))).into(),
                        Err(_) => return Err("invalid regex"),
                    },
                    "!~" => match Regex::new(s) {
                        Ok(r) => Restrict::not(restrict_fn(Some(Str::Regex(r)))),
                        Err(_) => return Err("invalid regex"),
                    },
                    _ => return Err("invalid string operator"),
                };

                Ok(r)
            }

        rule expr() -> Restrict
            = " "* invert:"!"? r:(attr_optional() / str_restrict() / str_restrict_optional()) " "* {
                let mut restrict = r;
                if invert.is_some() {
                    restrict = Restrict::not(restrict);
                }
                restrict
            }

        rule and() -> Restrict
            = "(" exprs:query() ++ "&&" ")" {
                Restrict::and(exprs)
            }

        rule or() -> Restrict
            = "(" exprs:query() ++ "||" ")" {
                Restrict::or(exprs)
            }

        rule xor() -> Restrict
            = "(" exprs:query() ++ "^^" ")" {
                Restrict::xor(exprs)
            }

        pub(super) rule query() -> Restrict
            = r:(expr() / and() / or() / xor()) { r }
    }
}

/// Convert a package query string into a Restriction.
pub fn pkg(s: &str) -> crate::Result<Restrict> {
    restrict::query(s).map_err(|e| peg_error(format!("invalid package query: {s:?}"), s, e))
}
