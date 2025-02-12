use once_cell::sync::Lazy;
use scallop::builtins::{Builtin, ExecStatus};
use scallop::{Error, Result};

use super::{eapply::run as eapply, PkgBuiltin};
use crate::pkgsh::BUILD_DATA;

const LONG_DOC: &str = "Apply user patches.";

#[doc = stringify!(LONG_DOC)]
pub(crate) fn run(args: &[&str]) -> Result<ExecStatus> {
    if !args.is_empty() {
        return Err(Error::Builtin(format!("takes no args, got {}", args.len())));
    }

    BUILD_DATA.with(|d| -> Result<ExecStatus> {
        if !d.borrow().user_patches_applied {
            let patches = &d.borrow().user_patches;
            let args: Vec<&str> = patches.iter().map(|s| s.as_str()).collect();
            if !args.is_empty() {
                eapply(&args)?;
            }
            d.borrow_mut().user_patches_applied = true;
        }
        Ok(ExecStatus::Success)
    })
}

pub(super) static BUILTIN: Lazy<PkgBuiltin> = Lazy::new(|| {
    PkgBuiltin::new(
        Builtin {
            name: "eapply_user",
            func: run,
            help: LONG_DOC,
            usage: "eapply_user",
        },
        &[("6-", &["src_prepare"])],
    )
});

#[cfg(test)]
mod tests {
    use super::super::assert_invalid_args;
    use super::run as eapply_user;

    #[test]
    fn invalid_args() {
        assert_invalid_args(eapply_user, &[1]);
    }
}
