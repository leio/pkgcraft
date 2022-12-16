use std::io::Write;

use scallop::builtins::ExecStatus;
use scallop::Error;

use crate::pkgsh::write_stderr;

use super::super::unescape::unescape;
use super::{make_builtin, ALL};

const LONG_DOC: &str = "Display information message when starting a process.";

#[doc = stringify!(LONG_DOC)]
pub(crate) fn run(args: &[&str]) -> scallop::Result<ExecStatus> {
    let (ret, args) = match args {
        [] => Err(Error::Base("requires 1 or more args, got 0".into())),
        [n, args @ ..] => match n.parse::<i32>() {
            Err(_) => Err(Error::Base(format!("invalid return value: {n}"))),
            Ok(ret) => Ok((ExecStatus::from(ret), args)),
        },
    }?;

    // TODO: support column-based formatting for success/failure indicators
    if bool::from(&ret) {
        write_stderr!("[ ok ]\n")?;
    } else {
        if !args.is_empty() {
            let unescaped: Result<Vec<_>, _> = args.iter().map(|s| unescape(s)).collect();
            let msg = unescaped?.join(" ");
            write_stderr!("{msg} ")?;
        }
        write_stderr!("[ !! ]\n")?;
    }

    Ok(ret)
}

const USAGE: &str = "eend $?";
make_builtin!("eend", eend_builtin, run, LONG_DOC, USAGE, &[("..", &[ALL])]);

#[cfg(test)]
mod tests {
    use crate::macros::assert_err_re;
    use crate::pkgsh::assert_stderr;

    use super::super::{assert_invalid_args, builtin_scope_tests};
    use super::run as eend;
    use super::*;

    builtin_scope_tests!(USAGE);

    #[test]
    fn invalid_args() {
        assert_invalid_args(eend, &[0]);
    }

    #[test]
    fn non_numeric_return_code() {
        assert_err_re!(eend(&["msg"]), "^invalid return value: msg$");
        assert_err_re!(eend(&["a", "msg"]), "^invalid return value: a$");
    }

    #[test]
    fn output() {
        for (args, expected) in [
            (vec!["0"], "[ ok ]\n"),
            (vec!["0", "msg"], "[ ok ]\n"),
            (vec!["0", "msg1", "msg2"], "[ ok ]\n"),
            (vec!["1"], "[ !! ]\n"),
            (vec!["1", "msg"], "msg [ !! ]\n"),
            (vec!["1", r"\tmsg"], "\tmsg [ !! ]\n"),
            (vec!["1", "msg1", "msg2"], "msg1 msg2 [ !! ]\n"),
        ] {
            eend(&args).unwrap();
            assert_stderr!(expected);
        }
    }
}
