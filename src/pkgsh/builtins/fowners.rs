use std::path::Path;
use std::process::Command;

use once_cell::sync::Lazy;
use scallop::builtins::{Builtin, ExecStatus};
use scallop::{Error, Result};

use super::PkgBuiltin;
use crate::command::RunCommand;
use crate::pkgsh::BUILD_DATA;

const LONG_DOC: &str = "Run `chown` taking paths relative to the image directory.";

#[doc = stringify!(LONG_DOC)]
pub(crate) fn run(args: &[&str]) -> Result<ExecStatus> {
    if args.len() < 2 {
        return Err(Error::Builtin(format!("requires at least 2 args, got {}", args.len())));
    }

    BUILD_DATA.with(|d| -> Result<ExecStatus> {
        let env = &d.borrow().env;
        let destdir = Path::new(
            env.get("ED")
                .unwrap_or_else(|| env.get("D").expect("$D undefined")),
        );

        let mut chown = Command::new("chown");
        for arg in args {
            let path = Path::new(arg.trim_start_matches('/'));
            chown.arg(destdir.join(path));
        }

        chown.run()?;

        Ok(ExecStatus::Success)
    })
}

pub(super) static BUILTIN: Lazy<PkgBuiltin> = Lazy::new(|| {
    PkgBuiltin::new(
        Builtin {
            name: "fowners",
            func: run,
            help: LONG_DOC,
            usage: "fowners user:group /path/to/file",
        },
        &[("0-", &["src_install", "pkg_preinst", "pkg_postinst"])],
    )
});

#[cfg(test)]
mod tests {
    use rusty_fork::rusty_fork_test;

    use super::super::assert_invalid_args;
    use super::run as fowners;

    rusty_fork_test! {
        #[test]
        fn invalid_args() {
            assert_invalid_args(fowners, &[0, 1]);
        }
    }
}
