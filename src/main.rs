mod format;
mod run;
mod state;

use crate::format::CodeStr;
use byte_unit::Byte;
use clap::{Command, arg};
use env_logger::{fmt::Color, Builder};
use log::{Level, LevelFilter};
use time::{OffsetDateTime, format_description};
use std::{
    env,
    io::{self, Write, IsTerminal, stderr},
    str::FromStr, time::Duration,
};
use tokio::time::sleep;

#[macro_use]
extern crate log;

// The program version
const VERSION: &str = env!("CARGO_PKG_VERSION");

// Defaults
const DEFAULT_LOG_LEVEL: LevelFilter = LevelFilter::Info;
const DEFAULT_THRESHOLD: &str = "10 GB";

// This struct represents the command-line arguments.
pub struct Settings {
    threshold: Byte,
}

// Set up the logger.
fn set_up_logging() {
    Builder::new()
        .filter_module(
            module_path!(),
            LevelFilter::from_str(
                &env::var("LOG_LEVEL").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.to_string()),
            )
            .unwrap_or_else(|_| DEFAULT_LOG_LEVEL),
        )
        .format(|buf, record| {
            let mut style = buf.style();
            style.set_bold(true);
            match record.level() {
                Level::Error => {
                    style.set_color(Color::Red);
                }
                Level::Warn => {
                    style.set_color(Color::Yellow);
                }
                Level::Info => {
                    style.set_color(Color::Green);
                }
                Level::Debug | Level::Trace => {
                    style.set_color(Color::Blue);
                }
            }
            let date_format = format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour]:[offset_minute]").unwrap();
            writeln!(
                buf,
                "{} {}",
                style.value(format!(
                    "[{} {}]",
                    OffsetDateTime::now_utc().format(&date_format).unwrap(),
                    record.level()
                )),
                record.args().to_string()
            )
        })
        .init();
}

// Parse the command-line arguments.
fn settings() -> io::Result<Settings> {
    // Set up the command-line interface.
    let matches = Command::new("Docuum")
        .version(VERSION)
        .author("Stephan Boyer <stephan@stephanboyer.com>")
        .about("Docuum performs LRU cache eviction for Docker images.")
        .arg(arg!(--threshold <VALUE>))
        .get_matches();

    // Read the threshold.
    let default_threshold = Byte::from_str(DEFAULT_THRESHOLD).unwrap(); // Manually verified safe
    matches.get_one::<Byte>("threshold")
        .or_else(|| Some(&default_threshold))
        .map(|threshold| Settings{ threshold: threshold.to_owned() })
        .ok_or( io::Error::new( io::ErrorKind::Other, "Invalid threshold {}."))
}

#[tokio::main]
// Let the fun begin!
async fn main() {
    // Determine whether to print colored output.
    colored::control::set_override(stderr().is_terminal());

    // Set up the logger.
    set_up_logging();

    // Parse the command-line arguments.
    match settings() {
        Ok(settings) => {
            // Try to load the state from disk.
            let mut state = state::load().unwrap_or_else(|error| {
                // We couldn't load any state from disk. Log the error.
                debug!(
                    "Unable to load state from disk. Proceeding with initial state. Details: {}",
                    error.to_string().code_str()
                );

                // Start with the initial state.
                state::initial()
            });

            // Stream Docker events and vacuum when necessary. Restart if an error occurs.
            loop {
                if let Err(e) = run::run(&settings, &mut state).await {
                    error!("{}", e);
                    info!("Restarting\u{2026}");
                    sleep(Duration::from_secs(1)).await;
                }
            }
        },
            Err(error) => {
                error!("{}", error);
                std::process::exit(1);
        }
    }

}
