use peg;

use crate::atom::ParseError;
use super::{DepSpec, Uri};
use crate::eapi::Eapi;
use crate::macros::opt_str;

peg::parser!{
    pub grammar src_uri() for str {
        rule _ = [' ']

        rule uri() -> &'input str
            = s:$(quiet!{!['(' | ')'] [^' ']+}) { s }

        rule useflag() -> &'input str
            = s:$(quiet!{
                ['a'..='z' | 'A'..='Z' | '0'..='9']
                ['a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '_' | '@' | '-']*
            } / expected!("useflag name")
            ) { s }

        rule uris(eapi: &'static Eapi) -> DepSpec
            = uris:uri() ++ " " {?
                let mut raw_uris = uris.iter().peekable();
                let mut uri_objs: Vec<Uri> = Vec::new();

                while let Some(x) = raw_uris.next() {
                    let rename = match raw_uris.peek() {
                        Some(&&"->") => {
                            if !eapi.has("src_uri_renames") {
                                return Err("URI renames are supported in >= EAPI 2");
                            }
                            raw_uris.next();
                            raw_uris.next().and_then(|s| opt_str!(s))
                        },
                        _ => None,
                    };
                    uri_objs.push(Uri { uri: x.to_string(), rename: rename });
                }

                Ok(DepSpec::Uris(uri_objs))
            }

        rule all_of(eapi: &'static Eapi) -> DepSpec
            = "(" _ e:expr(eapi) _ ")" {
                DepSpec::AllOf(Box::new(e))
            }

        // TODO: handle negation
        rule conditional(eapi: &'static Eapi) -> DepSpec
            = "!"? u:useflag() "?" _ "(" _ e:expr(eapi) _ ")" {
                DepSpec::ConditionalUse(u.to_string(), Box::new(e))
            }

        pub rule expr(eapi: &'static Eapi) -> DepSpec
            = conditional(eapi) / all_of(eapi) / uris(eapi)
    }
}

pub fn parse(s: &str, eapi: &'static Eapi) -> Result<DepSpec, ParseError> {
    src_uri::expr(s, eapi)
}

#[cfg(test)]
mod tests {
    use crate::atom::ParseError;
    use crate::depspec::{DepSpec, Uri};
    use crate::eapi::EAPI_LATEST;

    use super::src_uri::expr as parse;

    #[test]
    fn test_parse_src_uri() {
        // invalid data
        let mut result: Result<DepSpec, ParseError>;
        for s in [
                "", "(", ")", "( )", "( uri)", "| ( uri )", "use ( uri )", "!use ( uri )"
                ] {
            assert!(parse(&s, EAPI_LATEST).is_err(), "{} didn't fail", s);
        }

        let uri = |u1: &str, u2: Option<&str>| {
            Uri { uri: u1.to_string(), rename: u2.and_then(|s| Some(s.to_string())) }
        };

        // good data
        let mut src_uri;
        for (s, expected) in [
                ("uri1", DepSpec::Uris(vec![uri("uri1", None)])),
                ("uri1 uri2", DepSpec::Uris(vec![uri("uri1", None), uri("uri2", None)])),
                ("uri1 -> file", DepSpec::Uris(vec![uri("uri1", Some("file"))])),
                ] {
            result = parse(&s, EAPI_LATEST);
            assert!(result.is_ok(), "{} failed: {}", s, result.err().unwrap());
            src_uri = result.unwrap();
            assert_eq!(src_uri, expected);
        }
    }
}
