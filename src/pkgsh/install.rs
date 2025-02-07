use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, io};

use clap::Parser;
use filetime::{set_file_times, FileTime};
use itertools::{Either, Itertools};
use nix::{sys::stat, unistd};
use scallop::{Error, Result};
use walkdir::{DirEntry, WalkDir};

use super::BuildData;
use crate::command::RunCommand;
use crate::files::{Group, Mode, User};

#[derive(Parser, Debug, Default)]
#[clap(name = "install")]
struct InstallOptions {
    #[clap(short, long)]
    group: Option<Group>,
    #[clap(short, long)]
    owner: Option<User>,
    #[clap(short, long)]
    mode: Option<Mode>,
    #[clap(short, long)]
    preserve_timestamps: bool,
}

enum InstallOpts {
    None,
    Internal(InstallOptions),
    Cmd(Vec<String>),
}

impl Default for InstallOpts {
    fn default() -> Self {
        InstallOpts::None
    }
}

#[derive(Default)]
pub(super) struct Install {
    destdir: PathBuf,
    file_options: InstallOpts,
    dir_options: InstallOpts,
}

impl Install {
    pub(super) fn new(build: &BuildData) -> Self {
        let destdir = PathBuf::from(
            build
                .env
                .get("ED")
                .unwrap_or_else(|| build.env.get("D").expect("$D undefined")),
        );

        Install {
            destdir,
            ..Default::default()
        }
    }

    /// Set the target directory to install files into.
    pub(super) fn dest<P: AsRef<Path>>(mut self, dest: P) -> Result<Self> {
        let dest = dest.as_ref();
        self.dirs([dest])?;
        self.destdir.push(dest.strip_prefix("/").unwrap_or(dest));
        Ok(self)
    }

    fn parse_options<I, T>(&self, options: I) -> InstallOpts
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        let options: Vec<String> = options.into_iter().map(|s| s.into()).collect();
        let mut to_parse = vec!["install"];
        to_parse.extend(options.iter().map(|s| s.as_str()));

        match InstallOptions::try_parse_from(&to_parse) {
            Ok(opts) => InstallOpts::Internal(opts),
            Err(_) => InstallOpts::Cmd(options),
        }
    }

    /// Parse options to use for file attributes during install.
    pub(super) fn file_options<I, T>(mut self, options: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        self.file_options = self.parse_options(options);
        self
    }

    /// Parse options to use for dir attributes during install.
    pub(super) fn dir_options<I, T>(mut self, options: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        self.dir_options = self.parse_options(options);
        self
    }

    /// Prefix a given path with the target directory.
    pub(super) fn prefix<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        let path = path.as_ref();
        self.destdir.join(path.strip_prefix("/").unwrap_or(path))
    }

    /// Create a soft or hardlink between a given source and target.
    pub(super) fn link<F, P, Q>(&self, link: F, source: P, target: Q) -> Result<()>
    where
        F: Fn(&Path, &Path) -> io::Result<()>,
        P: AsRef<Path>,
        Q: AsRef<Path>,
    {
        let (source, target) = (source.as_ref(), target.as_ref());

        // create parent dirs
        if let Some(parent) = target.parent() {
            self.dirs([parent])?;
        }

        // capture target value before it's prefixed
        let failed = |e: io::Error| {
            return Err(Error::Base(format!(
                "failed creating link: {source:?} -> {target:?}: {e}"
            )));
        };

        let target = self.prefix(target);

        // overwrite link if it exists
        loop {
            match link(source, &target) {
                Ok(_) => break,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    fs::remove_file(&target).or_else(failed)?
                }
                Err(e) => return failed(e),
            }
        }

        Ok(())
    }

    fn set_attributes<P: AsRef<Path>>(&self, opts: &InstallOptions, path: P) -> Result<()> {
        let path = path.as_ref();
        let uid = opts.owner.as_ref().map(|o| o.uid);
        let gid = opts.group.as_ref().map(|g| g.gid);
        if uid.is_some() || gid.is_some() {
            unistd::fchownat(None, path, uid, gid, unistd::FchownatFlags::NoFollowSymlink)
                .map_err(|e| Error::Base(format!("failed setting file uid/gid: {path:?}: {e}")))?;
        }

        if let Some(mode) = &opts.mode {
            if !path.is_symlink() {
                stat::fchmodat(None, path, **mode, stat::FchmodatFlags::FollowSymlink)
                    .map_err(|e| Error::Base(format!("failed setting file mode: {path:?}: {e}")))?;
            }
        }

        Ok(())
    }

    /// Create given directories under the target directory.
    pub(super) fn dirs<I, P>(&self, paths: I) -> Result<()>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        match self.dir_options {
            InstallOpts::Cmd(_) => self.dirs_cmd(paths),
            _ => self.dirs_internal(paths),
        }
    }

    // Create directories using internal functionality.
    fn dirs_internal<I, P>(&self, paths: I) -> Result<()>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        for p in paths.into_iter() {
            let path = self.prefix(p);
            fs::create_dir_all(&path)
                .map_err(|e| Error::Base(format!("failed creating dir: {path:?}: {e}")))?;
            if let InstallOpts::Internal(opts) = &self.dir_options {
                self.set_attributes(opts, path)?;
            }
        }
        Ok(())
    }

    // Create directories using the `install` command.
    fn dirs_cmd<I, P>(&self, paths: I) -> Result<()>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut install = Command::new("install");
        install.arg("-d");
        if let InstallOpts::Cmd(opts) = &self.dir_options {
            install.args(opts);
        }
        install.args(paths.into_iter().map(|p| self.prefix(p)));
        install
            .run()
            .map_or_else(|e| Err(Error::Base(e.to_string())), |_| Ok(()))
    }

    /// Copy file trees under given directories to the target directory.
    pub(super) fn recursive<I, P, F>(&self, dirs: I, predicate: Option<F>) -> Result<()>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
        F: Fn(&DirEntry) -> bool,
    {
        for dir in dirs.into_iter() {
            let dir = dir.as_ref();
            // Determine whether to skip the base directory, path.components() can't be used
            // because it normalizes all occurrences of '.' away.
            let depth = match dir.to_string_lossy().ends_with("/.") {
                true => 1,
                false => 0,
            };

            // optionally apply directory filtering
            let entries = WalkDir::new(&dir).min_depth(depth);
            let entries = match predicate.as_ref() {
                None => Either::Left(entries),
                Some(func) => Either::Right(entries.into_iter().filter_entry(func)),
            };

            for entry in entries.into_iter() {
                let entry =
                    entry.map_err(|e| Error::Base(format!("error walking {dir:?}: {e}")))?;
                let path = entry.path();
                // TODO: replace with advance_by() once it's stable
                let dest = match depth {
                    0 => path,
                    n => {
                        let mut comp = path.components();
                        for _ in 0..n {
                            comp.next();
                        }
                        comp.as_path()
                    }
                };
                match path.is_dir() {
                    true => self.dirs([dest])?,
                    false => self.files_map([(path, dest)])?,
                }
            }
        }
        Ok(())
    }

    /// Install files from their given paths to the target directory.
    pub(super) fn files<'a, I, P>(&self, paths: I) -> Result<()>
    where
        I: IntoIterator<Item = &'a P>,
        P: AsRef<Path> + 'a + ?Sized,
    {
        let files = paths
            .into_iter()
            .map(|p| p.as_ref())
            .filter_map(|p| p.file_name().map(|name| (p, name)));

        match self.file_options {
            InstallOpts::Cmd(_) => self.files_cmd(files),
            _ => self.files_internal(files),
        }
    }

    /// Install files using a custom source -> dest mapping to the target directory.
    pub(super) fn files_map<I, P, Q>(&self, paths: I) -> Result<()>
    where
        I: IntoIterator<Item = (P, Q)>,
        P: AsRef<Path>,
        Q: AsRef<Path>,
    {
        match self.file_options {
            InstallOpts::Cmd(_) => self.files_cmd(paths),
            _ => self.files_internal(paths),
        }
    }

    // Install files using internal functionality.
    fn files_internal<I, P, Q>(&self, paths: I) -> Result<()>
    where
        I: IntoIterator<Item = (P, Q)>,
        P: AsRef<Path>,
        Q: AsRef<Path>,
    {
        for (source, dest) in paths.into_iter() {
            let source = source.as_ref();
            let dest = self.prefix(dest.as_ref());
            let meta = fs::metadata(source)
                .map_err(|e| Error::Base(format!("invalid file {source:?}: {e}")))?;

            // matching `install` command, remove dest before install
            match fs::remove_file(&dest) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => {
                    return Err(Error::Base(format!("failed removing file: {dest:?}: {e}")));
                }
                _ => (),
            }

            fs::copy(source, &dest).map_err(|e| {
                Error::Base(format!("failed copying file: {source:?} to {dest:?}: {e}"))
            })?;
            if let InstallOpts::Internal(opts) = &self.file_options {
                self.set_attributes(opts, &dest)?;
                if opts.preserve_timestamps {
                    let atime = FileTime::from_last_access_time(&meta);
                    let mtime = FileTime::from_last_modification_time(&meta);
                    set_file_times(&dest, atime, mtime)
                        .map_err(|e| Error::Base(format!("failed setting file time: {e}")))?;
                }
            }
        }
        Ok(())
    }

    // Install files using the `install` command.
    fn files_cmd<I, P, Q>(&self, paths: I) -> Result<()>
    where
        I: IntoIterator<Item = (P, Q)>,
        P: AsRef<Path>,
        Q: AsRef<Path>,
    {
        let mut files = Vec::<(PathBuf, PathBuf)>::new();
        for (source, dest) in paths.into_iter() {
            let source = source.as_ref();
            let dest = dest.as_ref();
            if source.is_symlink() {
                // install symlinks separately since `install` forcibly resolves them
                let source = fs::read_link(source).unwrap();
                self.link(|p, q| symlink(p, q), source, dest)?;
            } else {
                files.push((source.into(), self.prefix(dest)));
            }
        }

        // group and install sets of files by destination to decrease `install` calls
        let files_to_install: Vec<(&Path, &Path)> = files
            .iter()
            .map(|(p, q)| (p.as_path(), q.as_path()))
            .sorted_by_key(|x| x.1)
            .collect();
        for (dest, files_group) in &files_to_install.iter().group_by(|x| x.1) {
            let sources = files_group.map(|x| x.0);
            let mut install = Command::new("install");
            if let InstallOpts::Cmd(opts) = &self.file_options {
                install.args(opts);
            }
            install.args(sources);
            install.arg(dest);
            install.run().map(|_| ())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusty_fork::rusty_fork_test;

    use crate::command::{last_command, run_commands};
    use crate::macros::assert_err_re;
    use crate::pkgsh::test::FileTree;
    use crate::pkgsh::BUILD_DATA;

    rusty_fork_test! {
        #[test]
        fn nonexistent_file() {
            let _file_tree = FileTree::new();
            BUILD_DATA.with(|d| {
                let install = d.borrow().install();

                // nonexistent
                let r = install.files_internal([("source", "dest")]);
                assert_err_re!(r, format!("^invalid file \"source\": .*$"));
            })
        }

        #[test]
        fn dirs() {
            let file_tree = FileTree::new();
            BUILD_DATA.with(|d| {
                // internal dir creation is used for supported `install` options
                let install = d.borrow().install().dir_options(["-m0750"]);
                let mode = 0o40750;

                install.dirs(["dir"]).unwrap();
                file_tree.assert(format!(
                    r#"
                    [[files]]
                    path = "/dir"
                    mode = {mode}
                "#
                ));

                // use unhandled '-v' option to force `install` command usage
                let install = d.borrow().install().dir_options(["-v"]);

                install.dirs(["dir"]).unwrap();
                let cmd = last_command().unwrap();
                assert_eq!(cmd[..3], ["install", "-d", "-v"]);
            })
        }

        #[test]
        fn dirs_internal() {
            let file_tree = FileTree::new();
            BUILD_DATA.with(|d| {
                let install = d.borrow().install();
                let default_mode = 0o40755;

                // single dir
                install.dirs_internal(["dir"]).unwrap();
                file_tree.assert(format!(
                    r#"
                    [[files]]
                    path = "/dir"
                    mode = {default_mode}
                "#
                ));

                // multiple dirs
                install.dirs_internal(["a", "b"]).unwrap();
                file_tree.assert(format!(
                    r#"
                    [[files]]
                    path = "/a"
                    [[files]]
                    path = "/b"
                "#
                ));
            })
        }

        #[test]
        fn dirs_cmd() {
            let file_tree = FileTree::new();
            BUILD_DATA.with(|d| {
                let install = d.borrow().install();
                let default_mode = 0o40755;

                run_commands(|| {
                    // single dir
                    install.dirs_cmd(["dir"]).unwrap();
                    file_tree.assert(format!(
                        r#"
                        [[files]]
                        path = "/dir"
                        mode = {default_mode}
                    "#
                    ));

                    // multiple dirs
                    install.dirs_cmd(["a", "b"]).unwrap();
                    file_tree.assert(format!(
                        r#"
                        [[files]]
                        path = "/a"
                        [[files]]
                        path = "/b"
                    "#
                    ));
                });
            })
        }

        #[test]
        fn files() {
            let file_tree = FileTree::new();
            BUILD_DATA.with(|d| {
                // internal file creation is used for supported `install` options
                let install = d.borrow().install().file_options(["-m0750"]);
                let mode = 0o100750;

                // single file
                fs::File::create("file").unwrap();
                install.files(["file"]).unwrap();
                file_tree.assert(format!(
                    r#"
                    [[files]]
                    path = "/file"
                    mode = {mode}
                "#
                ));

                // single file mapping
                fs::File::create("src").unwrap();
                install.files_map([("src", "dest")]).unwrap();
                file_tree.assert(format!(
                    r#"
                    [[files]]
                    path = "/dest"
                    mode = {mode}
                "#
                ));

                // use unhandled '-v' option to force `install` command usage
                let install = d.borrow().install().file_options(["-v"]);

                // single file
                fs::File::create("file").unwrap();
                install.files(["file"]).unwrap();
                let cmd = last_command().unwrap();
                assert_eq!(cmd[..3], ["install", "-v", "file"]);

                // single file mapping
                fs::File::create("src").unwrap();
                install.files_map([("src", "dest")]).unwrap();
                let cmd = last_command().unwrap();
                assert_eq!(cmd[..3], ["install", "-v", "src"]);
            })
        }

        #[test]
        fn files_internal() {
            let file_tree = FileTree::new();
            BUILD_DATA.with(|d| {
                let install = d.borrow().install();
                let default_mode = 0o100644;

                // single file
                fs::File::create("src").unwrap();
                install.files_internal([("src", "dest")]).unwrap();
                file_tree.assert(format!(
                    r#"
                    [[files]]
                    path = "/dest"
                    mode = {default_mode}
                "#
                ));

                // multiple files
                fs::File::create("src1").unwrap();
                fs::File::create("src2").unwrap();
                install
                    .files_internal([("src1", "dest1"), ("src2", "dest2")])
                    .unwrap();
                file_tree.assert(format!(
                    r#"
                    [[files]]
                    path = "/dest1"
                    [[files]]
                    path = "/dest2"
                "#
                ));
            })
        }

        #[test]
        fn files_cmd() {
            let file_tree = FileTree::new();
            BUILD_DATA.with(|d| {
                let install = d.borrow().install();
                let default_mode = 0o100755;

                run_commands(|| {
                    // single file
                    fs::File::create("src").unwrap();
                    install.files_cmd([("src", "dest")]).unwrap();
                    file_tree.assert(format!(
                        r#"
                        [[files]]
                        path = "/dest"
                        mode = {default_mode}
                    "#
                    ));

                    // multiple files
                    fs::File::create("src1").unwrap();
                    fs::File::create("src2").unwrap();
                    install
                        .files_cmd([("src1", "dest1"), ("src2", "dest2")])
                        .unwrap();
                    file_tree.assert(format!(
                        r#"
                        [[files]]
                        path = "/dest1"
                        [[files]]
                        path = "/dest2"
                    "#
                    ));
                });
            })
        }
    }
}
