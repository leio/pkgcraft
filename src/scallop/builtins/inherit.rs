use scallop::builtins::{output_error_func, Builtin};
use scallop::variables::{array_to_vec, string_vec, unbind, ScopedVariable};
use scallop::{source, Result};

use crate::scallop::BUILD_DATA;

static LONG_DOC: &str = "\
Checks the value of the shell’s pipe status variable, and if any component is non-zero
(indicating failure), calls die, passing any parameters to it.";

#[doc = stringify!(LONG_DOC)]
pub(crate) fn run(args: &[&str]) -> Result<i32> {
    let eclasses: Vec<String> = args.iter().map(|s| s.to_string()).collect();

    BUILD_DATA.with(|d| -> Result<i32> {
        let eclass_var = ScopedVariable::new("ECLASS");
        let eapi = d.borrow().eapi;

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
            for var in &eapi.incremental_keys {
                unbind(var)?;
            }

            eclass_var.bind(&eclass, None);
            source::file(&format!("{}/eclass/{}.eclass", d.borrow().repo, &eclass)).unwrap();

            let mut d = d.borrow_mut();
            // append metadata keys that incrementally accumulate
            for var in &eapi.incremental_keys {
                if let Some(data) = string_vec(var) {
                    let deque = d.get_deque(var);
                    deque.extend(data);
                }
            }

            d.inherited.insert(eclass);
        }

        // unset metadata keys that incrementally accumulate
        for var in &eapi.incremental_keys {
            unbind(var)?;
        }

        Ok(0)
    })
}

pub static BUILTIN: Builtin = Builtin {
    name: "inherit",
    func: run,
    help: LONG_DOC,
    usage: "inherit eclass1 eclass2",
    error_func: Some(output_error_func),
};
