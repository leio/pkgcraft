use once_cell::sync::Lazy;
use scallop::builtins::{Builtin, ExecStatus};
use scallop::Result;

use super::_new::new;
use super::doinitd::run as doinitd;
use super::PkgBuiltin;

const LONG_DOC: &str = "Install renamed init scripts into /etc/init.d/.";

#[doc = stringify!(LONG_DOC)]
pub(crate) fn run(args: &[&str]) -> Result<ExecStatus> {
    new(args, doinitd)
}

pub(super) static BUILTIN: Lazy<PkgBuiltin> = Lazy::new(|| {
    PkgBuiltin::new(
        Builtin {
            name: "newinitd",
            func: run,
            help: LONG_DOC,
            usage: "newinitd path/to/init/file new_filename",
        },
        &[("0-", &["src_install"])],
    )
});

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use rusty_fork::rusty_fork_test;

    use super::super::assert_invalid_args;
    use super::run as newinitd;
    use crate::pkgsh::test::FileTree;
    use crate::pkgsh::write_stdin;

    rusty_fork_test! {
        #[test]
        fn invalid_args() {
            assert_invalid_args(newinitd, &[0, 1, 3]);
        }

        #[test]
        fn creation() {
            let file_tree = FileTree::new();

            fs::File::create("init").unwrap();
            newinitd(&["init", "pkgcraft"]).unwrap();
            file_tree.assert(r#"
                [[files]]
                path = "/etc/init.d/pkgcraft"
            "#);

            // re-run using data from stdin
            write_stdin!("pkgcraft");
            newinitd(&["-", "pkgcraft"]).unwrap();
            file_tree.assert(r#"
                [[files]]
                path = "/etc/init.d/pkgcraft"
                data = "pkgcraft"
            "#);
        }
    }
}
