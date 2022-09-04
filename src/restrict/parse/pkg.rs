use regex::Regex;

use crate::metadata::ebuild::MaintainerRestrict;
use crate::peg::peg_error;
use crate::pkg::ebuild::Restrict::{self as PkgRestrict, *};
use crate::restrict::{Restrict, Str};

peg::parser! {
    grammar restrict() for str {
        rule attr_optional() -> Restrict
            = attr:$(['a'..='z' | '_']+) " is " ("None" / "none") {?
                let r = match attr {
                    "raw_subslot" => RawSubslot(None),
                    "homepage" => Homepage(None),
                    "defined_phases" => DefinedPhases(None),
                    "keywords" => Keywords(None),
                    "iuse" => Iuse(None),
                    "inherit" => Inherit(None),
                    "inherited" => Inherited(None),
                    "long_description" => LongDescription(None),
                    "maintainers" => Maintainers(None),
                    "upstreams" => Upstreams(None),
                    _ => return Err("unknown optional package attribute"),
                };
                Ok(r.into())
            }

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
                    "raw_subslot" => |r: Str| -> PkgRestrict { RawSubslot(Some(r)) },
                    "long_description" => |r: Str| -> PkgRestrict { LongDescription(Some(r)) },
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

        rule maintainers() -> Restrict
            = "maintainers" r:maintainers_contains() { r }

        rule maintainers_attr_optional() -> MaintainerRestrict
            = attr:$(['a'..='z' | '_']+) " is " ("None" / "none") {?
                use crate::metadata::ebuild::MaintainerRestrict::*;
                let r = match attr {
                    "name" => Name(None),
                    "description" => Description(None),
                    "type" => Type(None),
                    "proxied" => Proxied(None),
                    _ => return Err("unknown optional maintainer attribute"),
                };
                Ok(r)
            }

        rule maintainers_str_restrict() -> MaintainerRestrict
            = attr:$(("email" / "name" / "description" / "type" / "proxied"))
                op:string_ops() s:quoted_string()
            {?
                use crate::metadata::ebuild::MaintainerRestrict::*;
                let restrict_fn = match attr {
                    "email" => Email,
                    "name" => |r: Str| -> MaintainerRestrict { Name(Some(r)) },
                    "description" => |r: Str| -> MaintainerRestrict { Description(Some(r)) },
                    "type" => |r: Str| -> MaintainerRestrict { Type(Some(r)) },
                    "proxied" => |r: Str| -> MaintainerRestrict { Proxied(Some(r)) },
                    _ => return Err("unknown maintainer attribute"),
                };

                let r = match op {
                    "==" => restrict_fn(Str::matches(s)),
                    "!=" => restrict_fn(Str::not(Str::matches(s))),
                    "=~" => match Regex::new(s) {
                        Ok(r) => restrict_fn(Str::Regex(r)),
                        Err(_) => return Err("invalid regex"),
                    },
                    "!~" => match Regex::new(s) {
                        Ok(r) => restrict_fn(Str::not(Str::Regex(r))),
                        Err(_) => return Err("invalid regex"),
                    },
                    _ => return Err("invalid string operator"),
                };

                Ok(r)
            }

        rule maintainers_contains() -> Restrict
            = " "+ "contains" " "+
                    r:(maintainers_attr_optional()
                       / maintainers_str_restrict()
                    ) {
                use crate::metadata::ebuild::SliceMaintainers::Contains;
                Contains(r).into()
            }

        rule expr() -> Restrict
            = " "* invert:"!"?
                    r:(attr_optional()
                       / str_restrict()
                       / maintainers()
                    ) " "* {
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
