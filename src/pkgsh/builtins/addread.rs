use once_cell::sync::Lazy;

use scallop::builtins::{Builtin, ExecStatus};
use scallop::Result;

use super::{PkgBuiltin, PHASE};

const LONG_DOC: &str = "Add a directory to the sandbox permitted read list.";

#[doc = stringify!(LONG_DOC)]
pub(crate) fn run(_args: &[&str]) -> Result<ExecStatus> {
    // TODO: fill out this stub
    Ok(ExecStatus::Success)
}

pub(super) static BUILTIN: Lazy<PkgBuiltin> = Lazy::new(|| {
    PkgBuiltin::new(
        Builtin {
            name: "addread",
            func: run,
            help: LONG_DOC,
            usage: "addread /sys",
        },
        &[("0-", &[PHASE])],
    )
});
