use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::{io, str};

use indexmap::IndexSet;
use scallop::builtins::{ExecStatus, ScopedOptions};
use scallop::variables::*;
use scallop::{functions, source, Error, Result};

use crate::eapi::Eapi;

pub mod builtins;
mod install;
pub(crate) mod phases;
#[cfg(test)]
pub(crate) mod test;
pub(crate) mod unescape;
mod utils;

#[cfg(not(test))]
type StdinType = io::Stdin;
#[cfg(test)]
type StdinType = io::Cursor<Vec<u8>>;

struct Stdin {
    inner: StdinType,
}

impl Default for Stdin {
    fn default() -> Self {
        #[cfg(not(test))]
        let inner = io::stdin();
        #[cfg(test)]
        let inner = io::Cursor::new(vec![]);

        Stdin { inner }
    }
}

#[cfg(test)]
macro_rules! write_stdin {
    ($($arg:tt)*) => {
        crate::pkgsh::BUILD_DATA.with(|d| {
            write!(d.borrow_mut().stdin.inner, $($arg)*).unwrap();
            d.borrow_mut().stdin.inner.set_position(0);
        })
    }
}
#[cfg(test)]
use write_stdin;

struct Stdout {
    #[cfg(not(test))]
    inner: io::Stdout,
    #[cfg(test)]
    inner: io::Cursor<Vec<u8>>,
}

impl Default for Stdout {
    fn default() -> Self {
        #[cfg(not(test))]
        let inner = io::stdout();
        #[cfg(test)]
        let inner = io::Cursor::new(vec![]);

        Stdout { inner }
    }
}

macro_rules! write_stdout {
    ($($arg:tt)*) => {
        crate::pkgsh::BUILD_DATA.with(|d| write!(d.borrow_mut().stdout.inner, $($arg)*).unwrap())
    }
}
use write_stdout;

#[cfg(test)]
macro_rules! get_stdout {
    () => {
        crate::pkgsh::BUILD_DATA.with(|d| {
            let mut d = d.borrow_mut();
            let output = std::str::from_utf8(d.stdout.inner.get_ref()).unwrap();
            let output = String::from(output);
            d.stdout.inner = std::io::Cursor::new(vec![]);
            output
        })
    };
}
#[cfg(test)]
use get_stdout;

#[cfg(test)]
macro_rules! assert_stdout {
    ($expected:expr) => {
        let output = crate::pkgsh::get_stdout!();
        assert_eq!(output, $expected);
    };
}
#[cfg(test)]
use assert_stdout;

struct Stderr {
    #[cfg(not(test))]
    inner: io::Stderr,
    #[cfg(test)]
    inner: io::Cursor<Vec<u8>>,
}

impl Default for Stderr {
    fn default() -> Self {
        #[cfg(not(test))]
        let inner = io::stderr();
        #[cfg(test)]
        let inner = io::Cursor::new(vec![]);

        Stderr { inner }
    }
}

macro_rules! write_stderr {
    ($($arg:tt)*) => {
        crate::pkgsh::BUILD_DATA.with(|d| write!(d.borrow_mut().stderr.inner, $($arg)*).unwrap())
    }
}
use write_stderr;

#[cfg(test)]
macro_rules! get_stderr {
    () => {
        crate::pkgsh::BUILD_DATA.with(|d| {
            let mut d = d.borrow_mut();
            let output = std::str::from_utf8(d.stderr.inner.get_ref()).unwrap();
            let output = String::from(output);
            d.stderr.inner = std::io::Cursor::new(vec![]);
            output
        })
    };
}
#[cfg(test)]
use get_stderr;

#[cfg(test)]
macro_rules! assert_stderr {
    ($expected:expr) => {
        let output = crate::pkgsh::get_stderr!();
        assert_eq!(output, $expected);
    };
}
#[cfg(test)]
use assert_stderr;

#[derive(Default)]
pub struct BuildData {
    pub repo: String,
    pub eapi: &'static Eapi,

    stdin: Stdin,
    stdout: Stdout,
    stderr: Stderr,

    // mapping of variables conditionally exported to the build environment
    pub env: HashMap<String, String>,

    // TODO: proxy these fields via borrowed package reference
    pub distfiles: Vec<String>,
    pub user_patches: Vec<String>,

    pub phase: String,
    pub phase_func: String,
    pub user_patches_applied: bool,

    pub desttree: String,
    pub docdesttree: String,
    pub exedesttree: String,
    pub insdesttree: String,

    pub insopts: Vec<String>,
    pub diropts: Vec<String>,
    pub exeopts: Vec<String>,
    pub libopts: Vec<String>,

    // TODO: add default values listed in the spec
    pub compress_include: HashSet<String>,
    pub compress_exclude: HashSet<String>,
    pub strip_include: HashSet<String>,
    pub strip_exclude: HashSet<String>,

    pub iuse_effective: HashSet<String>,
    pub use_: HashSet<String>,

    /// Eclasses directly inherited by an ebuild.
    pub inherit: Vec<String>,
    /// Full set of eclasses inherited by an ebuild.
    pub inherited: IndexSet<String>,

    // ebuild metadata fields
    pub iuse: VecDeque<String>,
    pub required_use: VecDeque<String>,
    pub depend: VecDeque<String>,
    pub rdepend: VecDeque<String>,
    pub pdepend: VecDeque<String>,
    pub bdepend: VecDeque<String>,
    pub idepend: VecDeque<String>,
    pub properties: VecDeque<String>,
    pub restrict: VecDeque<String>,
}

impl BuildData {
    fn new() -> Self {
        let mut data = BuildData::default();
        // set build state defaults
        data.insopts.push("-m0644".into());
        data.libopts.push("-m0644".into());
        data.diropts.push("-m0755".into());
        data.exeopts.push("-m0755".into());
        data.desttree = "/usr".into();
        data
    }

    fn stdin(&mut self) -> &mut StdinType {
        &mut self.stdin.inner
    }

    fn install(&self) -> install::Install {
        install::Install::new(self)
    }

    fn get_deque(&mut self, name: &str) -> &mut VecDeque<String> {
        match name {
            "IUSE" => &mut self.iuse,
            "REQUIRED_USE" => &mut self.required_use,
            "DEPEND" => &mut self.depend,
            "RDEPEND" => &mut self.rdepend,
            "PDEPEND" => &mut self.pdepend,
            "BDEPEND" => &mut self.bdepend,
            "IDEPEND" => &mut self.idepend,
            "PROPERTIES" => &mut self.properties,
            "RESTRICT" => &mut self.restrict,
            s => panic!("unknown field name: {s}"),
        }
    }
}

thread_local! {
    pub static BUILD_DATA: RefCell<BuildData> = RefCell::new(BuildData::new());
}

pub struct PkgShell<'a> {
    sh: &'a mut scallop::Shell,
}

impl<'a> PkgShell<'a> {
    pub fn new(sh: &'a mut scallop::Shell, data: BuildData) -> Self {
        // update thread local mutable for builtins
        BUILD_DATA.with(|d| d.replace(data));
        PkgShell { sh }
    }

    pub fn run_phase<S: AsRef<str>>(&self, phase: S) -> Result<ExecStatus> {
        BUILD_DATA.with(|d| -> Result<ExecStatus> {
            let phase = phase.as_ref();
            let eapi = d.borrow().eapi;
            let mut phase_func = ScopedVariable::new("EBUILD_PHASE_FUNC");

            // enable phase builtins
            let _builtins = eapi.scoped_builtins(phase)?;

            if eapi.has("ebuild_phase_func") {
                phase_func.bind(phase, None, None)?;
            }

            // run user space pre-phase hooks
            if let Some(mut func) = functions::find(format!("pre_{phase}")) {
                func.execute(&[])?;
            }

            // run user space phase function, falling back to internal default
            match functions::find(phase) {
                Some(mut func) => func.execute(&[])?,
                None => match eapi.phases().get(phase) {
                    Some(func) => func()?,
                    None => return Err(Error::Base(format!("nonexistent phase: {phase}"))),
                },
            };

            // run user space post-phase hooks
            if let Some(mut func) = functions::find(format!("post_{phase}")) {
                func.execute(&[])?;
            }

            Ok(ExecStatus::Success)
        })
    }

    pub fn source_ebuild<P: AsRef<Path>>(&mut self, ebuild: P) -> Result<()> {
        let ebuild = ebuild.as_ref();
        if !ebuild.exists() {
            return Err(Error::Base(format!("nonexistent ebuild: {ebuild:?}")));
        }

        BUILD_DATA.with(|d| -> Result<()> {
            let eapi = d.borrow().eapi;
            let mut opts = ScopedOptions::new();

            // enable global builtins
            let _builtins = eapi.scoped_builtins("global")?;

            if eapi.has("global_failglob") {
                opts.toggle(&["failglob"], &[])?;
            }

            source::file(&ebuild)?;

            // TODO: export default for $S

            // set RDEPEND=DEPEND if RDEPEND is unset
            if eapi.has("rdepend_default") && string_value("RDEPEND").is_none() {
                let depend = string_value("DEPEND").unwrap_or_else(|| String::from(""));
                bind("RDEPEND", &depend, None, None)?;
            }

            // prepend metadata keys that incrementally accumulate to eclass values
            let mut d = d.borrow_mut();
            for var in eapi.incremental_keys() {
                if let Ok(data) = string_vec(var) {
                    let deque = d.get_deque(var);
                    // TODO: extend_left() should be implemented upstream for VecDeque
                    for item in data.into_iter().rev() {
                        deque.push_front(item);
                    }
                }
            }
            Ok(())
        })
    }

    pub fn reset(&mut self) {
        self.sh.reset()
    }
}
