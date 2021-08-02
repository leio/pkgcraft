use anyhow::{Context, Result};
use clap::{App, Arg, ArgMatches};

use crate::settings::Settings;

pub fn cmd() -> App<'static> {
    App::new("del")
        .about("unregister repo")
        .arg(
            Arg::new("repos")
                .required(true)
                .takes_value(true)
                .multiple_values(true)
                .value_name("REPO")
                .about("repos to remove"),
        )
        .arg(Arg::new("clean").short('c').about("remove repo files"))
}

pub fn run(args: &ArgMatches, settings: &mut Settings) -> Result<()> {
    let repos: Vec<&str> = args.values_of("repos").unwrap().collect();
    let clean = args.is_present("clean");
    settings
        .config
        .repos
        .del(&repos, clean)
        .context("failed removing repo(s)")
}


