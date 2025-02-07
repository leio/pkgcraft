use once_cell::sync::Lazy;
use scallop::builtins::{Builtin, ExecStatus};
use scallop::variables::{array_to_vec, string_vec, unbind, ScopedVariable, Variable, Variables};
use scallop::{source, Error, Result};

use super::{PkgBuiltin, GLOBAL};
use crate::pkgsh::BUILD_DATA;

const LONG_DOC: &str = "Sources the given list of eclasses.";

#[doc = stringify!(LONG_DOC)]
pub(crate) fn run(args: &[&str]) -> Result<ExecStatus> {
    if args.is_empty() {
        return Err(Error::Builtin("requires 1 or more args, got 0".into()));
    }

    let eclasses: Vec<String> = args.iter().map(|s| s.to_string()).collect();

    BUILD_DATA.with(|d| -> Result<ExecStatus> {
        let mut eclass_var = ScopedVariable::new("ECLASS");
        let mut inherited_var = Variable::new("INHERITED");
        let eapi = d.borrow().eapi;
        // enable eclass builtins
        let _builtins = eapi.scoped_builtins("eclass")?;

        // track direct ebuild inherits
        if let Ok(source) = array_to_vec("BASH_SOURCE") {
            if source.len() == 1 && source[0].ends_with(".ebuild") {
                d.borrow_mut().inherit.extend(eclasses.clone());
            }
        }

        for eclass in eclasses {
            // don't re-inherit eclasses
            if d.borrow().inherited.contains(&eclass) {
                continue;
            }

            // unset metadata keys that incrementally accumulate
            for var in eapi.incremental_keys() {
                unbind(var)?;
            }

            eclass_var.bind(&eclass, None, None)?;
            source::file(&format!("{}/eclass/{eclass}.eclass", d.borrow().repo)).unwrap();

            let mut d = d.borrow_mut();
            // append metadata keys that incrementally accumulate
            for var in eapi.incremental_keys() {
                if let Ok(data) = string_vec(var) {
                    let deque = d.get_deque(var);
                    deque.extend(data);
                }
            }

            inherited_var.append(&format!(" {eclass}"))?;
            d.inherited.insert(eclass);
        }

        // unset metadata keys that incrementally accumulate
        for var in eapi.incremental_keys() {
            unbind(var)?;
        }

        Ok(ExecStatus::Success)
    })
}

pub(super) static BUILTIN: Lazy<PkgBuiltin> = Lazy::new(|| {
    PkgBuiltin::new(
        Builtin {
            name: "inherit",
            func: run,
            help: LONG_DOC,
            usage: "inherit eclass1 eclass2",
        },
        &[("0-", &[GLOBAL])],
    )
});

#[cfg(test)]
mod tests {
    use super::super::assert_invalid_args;
    use super::run as inherit;

    #[test]
    fn invalid_args() {
        assert_invalid_args(inherit, &[0]);
    }
}
