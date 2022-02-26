use once_cell::sync::Lazy;
use scallop::builtins::{Builtin, ExecStatus};
use scallop::Result;

use super::PkgBuiltin;
use super::_default_phase_func::default_phase_func;

const LONG_DOC: &str = "\
Runs the default src_install implementation for a package's EAPI.";

#[doc = stringify!(LONG_DOC)]
pub(crate) fn run(args: &[&str]) -> Result<ExecStatus> {
    default_phase_func(args)
}

pub(super) static BUILTIN: Lazy<PkgBuiltin> = Lazy::new(|| {
    PkgBuiltin::new(
        Builtin {
            name: "default_src_install",
            func: run,
            help: LONG_DOC,
            usage: "default_src_install",
        },
        &[("4-", &["src_install"])],
    )
});

#[cfg(test)]
mod tests {
    use super::super::assert_invalid_args;
    use super::run as default_src_install;

    #[test]
    fn invalid_args() {
        assert_invalid_args(default_src_install, &[1]);
    }
}
