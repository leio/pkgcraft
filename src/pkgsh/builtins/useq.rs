use once_cell::sync::Lazy;
use scallop::builtins::{Builtin, ExecStatus};
use scallop::Result;

use super::use_::run as use_;
use super::{PkgBuiltin, PHASE};

const LONG_DOC: &str = "Deprecated synonym for use.";

#[doc = stringify!(LONG_DOC)]
pub(crate) fn run(args: &[&str]) -> Result<ExecStatus> {
    use_(args)
}

pub(super) static BUILTIN: Lazy<PkgBuiltin> = Lazy::new(|| {
    PkgBuiltin::new(
        Builtin {
            name: "useq",
            func: run,
            help: LONG_DOC,
            usage: "useq flag",
        },
        &[("0-7", &[PHASE])],
    )
});
