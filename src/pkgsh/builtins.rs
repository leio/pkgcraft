use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::Regex;
use scallop::builtins::{Builtin, ExecStatus};

use crate::{eapi, eapi::Eapi};

mod _default_phase_func;
mod _new;
mod _use_conf;
pub mod adddeny;
pub mod addpredict;
pub mod addread;
pub mod addwrite;
pub mod assert;
pub mod debug_print;
pub mod debug_print_function;
pub mod debug_print_section;
pub mod default;
pub mod default_pkg_nofetch;
pub mod default_src_compile;
pub mod default_src_configure;
pub mod default_src_install;
pub mod default_src_prepare;
pub mod default_src_test;
pub mod default_src_unpack;
pub mod die;
pub mod diropts;
pub mod dobin;
pub mod docinto;
pub mod docompress;
pub mod doconfd;
pub mod dodir;
pub mod dodoc;
pub mod doenvd;
pub mod doexe;
pub mod dohard;
pub mod doheader;
pub mod dohtml;
pub mod doinfo;
pub mod doinitd;
pub mod doins;
pub mod dolib;
pub mod dolib_a;
pub mod dolib_so;
pub mod doman;
pub mod domo;
pub mod dosbin;
pub mod dosed;
pub mod dostrip;
pub mod dosym;
pub mod eapply;
pub mod eapply_user;
pub mod ebegin;
pub mod econf;
pub mod eend;
pub mod eerror;
pub mod einfo;
pub mod einfon;
pub mod einstall;
pub mod einstalldocs;
pub mod emake;
pub mod eqawarn;
pub mod ewarn;
pub mod exeinto;
pub mod exeopts;
pub mod export_functions;
pub mod fowners;
pub mod fperms;
pub mod get_libdir;
pub mod has;
pub mod hasq;
pub mod hasv;
pub mod in_iuse;
pub mod inherit;
pub mod insinto;
pub mod insopts;
pub mod into;
pub mod keepdir;
pub mod libopts;
pub mod newbin;
pub mod newconfd;
pub mod newdoc;
pub mod newenvd;
pub mod newexe;
pub mod newheader;
pub mod newinitd;
pub mod newins;
pub mod newlib_a;
pub mod newlib_so;
pub mod newman;
pub mod newsbin;
pub mod nonfatal;
pub mod unpack;
pub mod use_;
pub mod use_enable;
pub mod use_with;
pub mod useq;
pub mod usev;
pub mod usex;
pub mod ver_cut;
pub mod ver_rs;
pub mod ver_test;

pub(crate) struct PkgBuiltin {
    builtin: Builtin,
    scope: IndexMap<&'static Eapi, Regex>,
}

// scope patterns
const ALL: &str = ".+";
const ECLASS: &str = "eclass";
const GLOBAL: &str = "global";
const PHASE: &str = ".+_.+";

impl PkgBuiltin {
    fn new(builtin: Builtin, scopes: &[(&str, &[&str])]) -> Self {
        let mut scope = IndexMap::new();
        for (eapis, s) in scopes.iter() {
            let scope_re = Regex::new(&format!(r"^{}$", s.join("|"))).unwrap();
            for e in eapi::supported(eapis).expect("failed to parse EAPI range") {
                if scope.insert(e, scope_re.clone()).is_some() {
                    panic!("clashing EAPI scopes: {e}");
                }
            }
        }
        PkgBuiltin { builtin, scope }
    }

    pub(crate) fn run(&self, args: &[&str]) -> scallop::Result<ExecStatus> {
        self.builtin.run(args)
    }
}

pub(crate) type BuiltinsMap = HashMap<&'static str, &'static PkgBuiltin>;
pub(crate) type ScopeBuiltinsMap = HashMap<String, BuiltinsMap>;
pub(crate) type EapiBuiltinsMap = HashMap<&'static Eapi, ScopeBuiltinsMap>;

// TODO: auto-generate the builtin module imports and vector creation via build script
pub(crate) static BUILTINS_MAP: Lazy<EapiBuiltinsMap> = Lazy::new(|| {
    let builtins: Vec<&PkgBuiltin> = vec![
        &adddeny::BUILTIN,
        &addpredict::BUILTIN,
        &addread::BUILTIN,
        &addwrite::BUILTIN,
        &assert::BUILTIN,
        &debug_print::BUILTIN,
        &debug_print_function::BUILTIN,
        &debug_print_section::BUILTIN,
        &default::BUILTIN,
        &default_pkg_nofetch::BUILTIN,
        &default_src_compile::BUILTIN,
        &default_src_configure::BUILTIN,
        &default_src_install::BUILTIN,
        &default_src_prepare::BUILTIN,
        &default_src_test::BUILTIN,
        &default_src_unpack::BUILTIN,
        &die::BUILTIN,
        &diropts::BUILTIN,
        &dobin::BUILTIN,
        &docinto::BUILTIN,
        &docompress::BUILTIN,
        &doconfd::BUILTIN,
        &dodir::BUILTIN,
        &dodoc::BUILTIN,
        &doman::BUILTIN,
        &domo::BUILTIN,
        &doenvd::BUILTIN,
        &doexe::BUILTIN,
        &dohard::BUILTIN,
        &doheader::BUILTIN,
        &dohtml::BUILTIN,
        &doinfo::BUILTIN,
        &doinitd::BUILTIN,
        &doins::BUILTIN,
        &dolib::BUILTIN,
        &dolib_a::BUILTIN,
        &dolib_so::BUILTIN,
        &dosed::BUILTIN,
        &dosbin::BUILTIN,
        &dostrip::BUILTIN,
        &dosym::BUILTIN,
        &eapply::BUILTIN,
        &eapply_user::BUILTIN,
        &ebegin::BUILTIN,
        &econf::BUILTIN,
        &eend::BUILTIN,
        &eerror::BUILTIN,
        &einfo::BUILTIN,
        &einfon::BUILTIN,
        &einstalldocs::BUILTIN,
        &einstall::BUILTIN,
        &emake::BUILTIN,
        &eqawarn::BUILTIN,
        &ewarn::BUILTIN,
        &exeinto::BUILTIN,
        &exeopts::BUILTIN,
        &export_functions::BUILTIN,
        &fowners::BUILTIN,
        &fperms::BUILTIN,
        &get_libdir::BUILTIN,
        &has::BUILTIN,
        &hasq::BUILTIN,
        &hasv::BUILTIN,
        &in_iuse::BUILTIN,
        &inherit::BUILTIN,
        &insinto::BUILTIN,
        &insopts::BUILTIN,
        &into::BUILTIN,
        &keepdir::BUILTIN,
        &libopts::BUILTIN,
        &newbin::BUILTIN,
        &newconfd::BUILTIN,
        &newdoc::BUILTIN,
        &newenvd::BUILTIN,
        &newexe::BUILTIN,
        &newheader::BUILTIN,
        &newinitd::BUILTIN,
        &newins::BUILTIN,
        &newlib_a::BUILTIN,
        &newlib_so::BUILTIN,
        &newman::BUILTIN,
        &newsbin::BUILTIN,
        &nonfatal::BUILTIN,
        &unpack::BUILTIN,
        &use_::BUILTIN,
        &use_enable::BUILTIN,
        &use_with::BUILTIN,
        &useq::BUILTIN,
        &usev::BUILTIN,
        &usex::BUILTIN,
        &ver_cut::BUILTIN,
        &ver_rs::BUILTIN,
        &ver_test::BUILTIN,
    ];

    let static_scopes: Vec<&str> = vec!["global", "eclass"];
    #[allow(clippy::mutable_key_type)]
    let mut builtins_map = EapiBuiltinsMap::new();
    for b in builtins.iter() {
        for (eapi, re) in b.scope.iter() {
            let scope_map = builtins_map
                .entry(eapi)
                .or_insert_with(ScopeBuiltinsMap::new);
            let phase_scopes = eapi.phases().keys();
            let scopes = static_scopes.iter().chain(phase_scopes);
            for scope in scopes.filter(|s| re.is_match(s)) {
                scope_map
                    .entry(scope.to_string())
                    .or_insert_with(BuiltinsMap::new)
                    .insert(b.builtin.name, b);
            }
        }
    }
    builtins_map
});

static NONFATAL: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

static VERSION_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?P<sep>[[:^alnum:]]+)?(?P<comp>[[:digit:]]+|[[:alpha:]]+)?").unwrap()
});

/// Split version string into a vector of separators and components.
fn version_split(ver: &str) -> Vec<&str> {
    let mut version_parts = Vec::new();
    for caps in VERSION_RE.captures_iter(ver) {
        version_parts.extend([
            caps.name("sep").map_or("", |m| m.as_str()),
            caps.name("comp").map_or("", |m| m.as_str()),
        ]);
    }
    version_parts
}

peg::parser! {
    grammar cmd() for str {
        // Parse ranges used with the ver_rs and ver_cut commands.
        pub rule range(max: usize) -> (usize, usize)
            = start_s:$(['0'..='9']+) "-" end_s:$(['0'..='9']+) {
                let start = start_s.parse::<usize>().unwrap();
                let end = end_s.parse::<usize>().unwrap();
                (start, end)
            } / start_s:$(['0'..='9']+) "-" {
                match start_s.parse::<usize>().unwrap() {
                    start if start <= max => (start, max),
                    start => (start, start),
                }
            } / start_s:$(['0'..='9']+) {
                let start = start_s.parse::<usize>().unwrap();
                (start, start)
            }
    }
}

// provide public parsing functionality while converting error types
pub(crate) mod parse {
    use crate::peg::peg_error;

    use super::cmd;
    use crate::{Error, Result};

    pub(crate) fn range(s: &str, max: usize) -> Result<(usize, usize)> {
        let (start, end) =
            cmd::range(s, max).map_err(|e| peg_error(format!("invalid range: {s:?}"), s, e))?;
        if end < start {
            return Err(Error::InvalidValue(format!(
                "start of range ({start}) is greater than end ({end})",
            )));
        }
        Ok((start, end))
    }
}

#[cfg(test)]
fn assert_invalid_args(func: ::scallop::builtins::BuiltinFn, nums: &[u32]) {
    for n in nums {
        let args: Vec<String> = (0..*n).map(|n| n.to_string()).collect();
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let re = format!("^.*, got {n}");
        crate::macros::assert_err_re!(func(&args), re);
    }
}
