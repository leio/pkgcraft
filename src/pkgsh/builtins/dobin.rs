use std::path::{Path, PathBuf};

use nix::unistd::geteuid;
use once_cell::sync::Lazy;
use scallop::builtins::{Builtin, ExecStatus};
use scallop::{Error, Result};

use super::PkgBuiltin;
use crate::pkgsh::BUILD_DATA;

const LONG_DOC: &str = "Install executables into DESTTREE/bin.";

pub(super) fn install_bin(args: &[&str], dest: &str) -> Result<ExecStatus> {
    BUILD_DATA.with(|d| -> Result<ExecStatus> {
        let dest: PathBuf = [&d.borrow().desttree, dest].iter().collect();
        let opts: &[&str] = match geteuid().is_root() {
            true => &["-m0755", "-o", "root", "-g", "root"],
            false => &["-m0755"],
        };
        let install = d
            .borrow()
            .install()
            .dest(&dest)?
            .ins_options(opts.iter().copied());

        let files = args
            .iter()
            .map(Path::new)
            .filter_map(|f| f.file_name().map(|name| (f, name)));
        install.files(files)?;

        Ok(ExecStatus::Success)
    })
}

#[doc = stringify!(LONG_DOC)]
pub(crate) fn run(args: &[&str]) -> Result<ExecStatus> {
    if args.is_empty() {
        return Err(Error::Builtin("requires 1 or more args, got 0".into()));
    }

    install_bin(args, "bin")
}

pub(super) static BUILTIN: Lazy<PkgBuiltin> = Lazy::new(|| {
    PkgBuiltin::new(
        Builtin {
            name: "dobin",
            func: run,
            help: LONG_DOC,
            usage: "dobin path/to/executable",
        },
        &[("0-", &["src_install"])],
    )
});

#[cfg(test)]
mod tests {
    use std::fs;

    use rusty_fork::rusty_fork_test;

    use super::super::assert_invalid_args;
    use super::super::exeopts::run as exeopts;
    use super::run as dobin;
    use crate::macros::assert_err_re;
    use crate::pkgsh::test::FileTree;

    rusty_fork_test! {
        #[test]
        fn invalid_args() {
            assert_invalid_args(dobin, &[0]);

            let _file_tree = FileTree::new();

            // nonexistent
            let r = dobin(&["pkgcraft"]);
            assert_err_re!(r, format!("^invalid file \"pkgcraft\": .*$"));
        }

        #[test]
        fn creation() {
            let file_tree = FileTree::new();
            let default_mode = 0o100755;

            fs::File::create("pkgcraft").unwrap();
            dobin(&["pkgcraft"]).unwrap();
            file_tree.assert(format!(r#"
                [[files]]
                path = "/usr/bin/pkgcraft"
                mode = {default_mode}
            "#));

            // verify exeopts are ignored
            exeopts(&["-m0777"]).unwrap();
            dobin(&["pkgcraft"]).unwrap();
            file_tree.assert(format!(r#"
                [[files]]
                path = "/usr/bin/pkgcraft"
                mode = {default_mode}
            "#));
        }
    }
}
