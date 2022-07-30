use once_cell::sync::Lazy;
use scallop::builtins::{make_builtin, ExecStatus};
use scallop::Result;

use super::PkgBuiltin;
use super::_default_phase_func::default_phase_func;

const LONG_DOC: &str = "\
Runs the default pkg_nofetch implementation for a package's EAPI.";

#[doc = stringify!(LONG_DOC)]
pub(crate) fn run(args: &[&str]) -> Result<ExecStatus> {
    default_phase_func(args)
}

make_builtin!(
    "default_pkg_nofetch",
    default_pkg_nofetch_builtin,
    run,
    LONG_DOC,
    "default_pkg_nofetch"
);

pub(super) static PKG_BUILTIN: Lazy<PkgBuiltin> =
    Lazy::new(|| PkgBuiltin::new(BUILTIN, &[("2-", &["pkg_nofetch"])]));

#[cfg(test)]
mod tests {
    use super::super::assert_invalid_args;
    use super::run as default_pkg_nofetch;

    #[test]
    fn invalid_args() {
        assert_invalid_args(default_pkg_nofetch, &[1]);
    }

    // TODO: add tests
}
