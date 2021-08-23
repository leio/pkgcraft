use std::io;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use clap::{App, AppSettings, Arg};
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

pub mod arcanist {
    tonic::include_proto!("arcanist");
}

use arcanist::arcanist_client::ArcanistClient;
use argparse::{positive_int, str_to_bool};
use settings::Settings;

mod argparse;
mod settings;
mod subcmds;

pub type Client = ArcanistClient<Channel>;

/// Try starting a local arcanist instance.
async fn start_arcanist(path: &Path, timeout: &u64) -> Result<()> {
    Command::new("arcanist")
        .spawn()
        .context("failed starting arcanist")?;
    // wait for arcanist to start
    let mut sleep_ms: u64 = 100;
    let timeout_ms: u64 = timeout * 1000;
    loop {
        thread::sleep(Duration::from_millis(sleep_ms));
        match UnixStream::connect(path).await {
            Ok(_) => return Ok(()),
            _ => {
                sleep_ms *= 2;
                if sleep_ms >= timeout_ms {
                    return Err(anyhow!("timed out starting arcanist"));
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let app = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .about("command-line tool leveraging pkgcraft")
        .setting(AppSettings::DisableHelpSubcommand)
        .setting(AppSettings::DisableVersionForSubcommands)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommands(subcmds::register())
        .arg(
            Arg::new("color")
                .long("color")
                .takes_value(true)
                .value_name("BOOLEAN")
                .validator(str_to_bool)
                .about("toggle colored output"),
        )
        .arg(Arg::new("debug").long("debug").about("enable debug output"))
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .multiple_occurrences(true)
                .about("enable verbose output"),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .multiple_occurrences(true)
                .about("suppress non-error messages"),
        )
        .arg(
            Arg::new("socket")
                .short('c')
                .long("connect")
                .value_name("URL")
                .about("connect to given arcanist instance"),
        )
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .value_name("SECONDS")
                .default_value("5")
                .validator(positive_int)
                .about("connection timeout"),
        );

    // determine subcommand being run to use for error output
    //let app_ignore_errors = app.clone().setting(AppSettings::IgnoreErrors);
    //let pre_parsed = app_ignore_errors.get_matches();

    let matches = app.get_matches();

    // load config settings and then override them with command-line settings
    let mut settings = Settings::new()?;

    if let Some(color) = matches.value_of("color") {
        settings.color = str_to_bool(color)?;
    }

    if matches.is_present("debug") {
        settings.debug = true;
    }
    settings.verbosity += matches.occurrences_of("verbose") as i32;
    settings.verbosity -= matches.occurrences_of("quiet") as i32;

    stderrlog::new()
        .modules([module_path!(), "pkgcraft"])
        .verbosity(matches.occurrences_of("verbose") as usize)
        .quiet(settings.verbosity < 0)
        .init()?;

    // load pkgcraft config
    settings.load()?;

    let socket = matches.value_of("socket").map(|s| s.to_string());
    let url = socket.clone().unwrap_or_else(|| "http://[::]".to_string());
    let timeout = matches
        .value_of("timeout")
        .unwrap_or_default()
        .parse::<u64>()
        .unwrap();
    let endpoint = Endpoint::from_shared(url)?
        .connect_timeout(Duration::from_secs(timeout))
        .timeout(Duration::from_secs(timeout));

    // connect to arcanist
    let channel: Channel = match socket {
        Some(socket) => endpoint
            .connect()
            .await
            .context(format!("failed connecting to arcanist: {:?}", &socket))?,
        None => {
            let path = settings.config.get_socket("arcanist.sock", false)?;
            if let Err(e) = UnixStream::connect(&path).await {
                match e.kind() {
                    io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound => {
                        start_arcanist(&path, &timeout).await?;
                    }
                    _ => (),
                }
            }
            endpoint
                .connect_with_connector(service_fn(move |_: Uri| UnixStream::connect(path.clone())))
                .await
                .context("failed connecting to arcanist")?
        }
    };

    let mut client: Client = ArcanistClient::new(channel);

    subcmds::run(&matches, &mut client, &mut settings).await
}
