use anyhow::Result;
use clap::{App, ArgMatches};

use crate::settings::Settings;
use crate::Client;

mod add;
mod del;
mod repo;
mod search;
mod version;

#[rustfmt::skip]
pub fn register() -> Vec<App<'static>> {
    vec![
        add::cmd(),
        del::cmd(),
        repo::cmd(),
        search::cmd(),
        version::cmd(),
    ]
}

pub async fn run(args: &ArgMatches, client: &mut Client, settings: &mut Settings) -> Result<()> {
    let (subcmd, m) = args.subcommand().unwrap();
    match subcmd {
        "add" => add::run(m, client),
        "del" => del::run(m, client),
        "repo" => repo::run(m, client, settings),
        "search" => search::run(m, client),
        "version" => version::run(client).await,
        _ => panic!("unknown subcommand"),
    }
}
