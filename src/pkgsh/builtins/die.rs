use std::io::Write;
use std::sync::atomic::Ordering;

use once_cell::sync::Lazy;
use scallop::builtins::{Builtin, ExecStatus};
use scallop::{Error, Result};

use crate::eapi::Feature;
use crate::pkgsh::{write_stderr, BUILD_DATA};

use super::{PkgBuiltin, ALL, NONFATAL};

const LONG_DOC: &str = "\
Displays a failure message provided in an optional argument and then aborts the build process.";

#[doc = stringify!(LONG_DOC)]
pub(crate) fn run(args: &[&str]) -> Result<ExecStatus> {
    BUILD_DATA.with(|d| -> Result<ExecStatus> {
        let eapi = d.borrow().eapi;
        let args = match args.len() {
            1 | 2 if eapi.has(Feature::NonfatalDie) && args[0] == "-n" => {
                if NONFATAL.load(Ordering::Relaxed) {
                    if args.len() == 2 {
                        write_stderr!("{}\n", args[1]);
                    }
                    return Ok(ExecStatus::Failure(1));
                }
                &args[1..]
            }
            0 | 1 => args,
            n => return Err(Error::Builtin(format!("takes up to 1 arg, got {n}"))),
        };

        let msg = match args.is_empty() {
            true => "(no error message)",
            false => args[0],
        };

        // TODO: add bash backtrace to output
        Err(Error::Bail(msg.to_string()))
    })
}

pub(super) static BUILTIN: Lazy<PkgBuiltin> = Lazy::new(|| {
    PkgBuiltin::new(
        Builtin {
            name: "die",
            func: run,
            help: LONG_DOC,
            usage: "die \"error message\"",
        },
        &[("0-", &[ALL])],
    )
});

#[cfg(test)]
mod tests {
    use scallop::variables::*;
    use scallop::{builtins, source};

    use crate::eapi::{Feature, EAPIS_OFFICIAL};
    use crate::macros::assert_err_re;
    use crate::pkgsh::BUILD_DATA;

    use super::super::assert_invalid_args;
    use super::run as die;

    #[test]
    fn invalid_args() {
        assert_invalid_args(die, &[3]);

        BUILD_DATA.with(|d| {
            for eapi in EAPIS_OFFICIAL
                .values()
                .filter(|e| !e.has(Feature::NonfatalDie))
            {
                d.borrow_mut().eapi = eapi;
                assert_invalid_args(die, &[2]);
            }
        });
    }

    #[test]
    fn main() {
        builtins::enable(&["die"]).unwrap();
        bind("VAR", "1", None, None).unwrap();

        let r = source::string("die && VAR=2");
        assert_err_re!(r, r"^die: error: \(no error message\)");

        // verify bash state
        assert_eq!(string_value("VAR").unwrap(), "1");

        // verify message output
        let r = source::string("die \"output message\"");
        assert_err_re!(r, r"^die: error: output message");
    }

    #[test]
    #[cfg_attr(target_os = "macos", ignore)] // TODO: debug bash failures
    fn subshell() {
        builtins::enable(&["die"]).unwrap();
        bind("VAR", "1", None, None).unwrap();

        let r = source::string("FOO=$(die); VAR=2");
        assert_err_re!(r, r"^die: error: \(no error message\)");

        // verify bash state
        assert_eq!(string_value("VAR").unwrap(), "1");

        // verify message output
        let r = source::string("VAR=$(die \"output message\")");
        assert_err_re!(r, r"^die: error: output message");
    }

    #[test]
    fn nonfatal() {
        builtins::enable(&["die", "nonfatal"]).unwrap();
        bind("VAR", "1", None, None).unwrap();

        // nonfatal requires `die -n` call
        let r = source::string("nonfatal die && VAR=2");
        assert_err_re!(r, r"^die: error: \(no error message\)");

        // nonfatal die in main process
        bind("VAR", "1", None, None).unwrap();
        source::string("nonfatal die -n && VAR=2").unwrap();
        assert_eq!(string_value("VAR").unwrap(), "2");

        // nonfatal die in subshell
        bind("VAR", "1", None, None).unwrap();
        source::string("FOO=$(nonfatal die -n); VAR=2").unwrap();
        assert_eq!(string_value("VAR").unwrap(), "2");
    }
}
