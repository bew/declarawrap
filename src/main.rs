// declarawrap — declarative binary wrapper.
// Reads a KDL config file and execs the wrapped command with its overrides applied.

#![expect(dead_code)]

mod config;
mod kdl_config_parser;

use std::path::Path;

use anyhow::Context;

/// CLI program name, used in usage and error messages.
static PROGRAM_NAME: &str = "declarawrap";

/// Print the usage line to stderr.
fn show_usage() {
    eprintln!("Usage: {PROGRAM_NAME} <config-file>");
}

#[derive(Debug)]
struct CliOptions {
    config_file: String,
}

/// Parse CLI arguments into [`CliOptions`].
///
/// Only the config file is accepted; any further arguments are noted as ignored.
/// Returns an error when the config file argument is missing.
fn parse_args() -> Result<CliOptions, String> {
    let mut given_args = std::env::args();
    given_args.next(); // skip program name/path
    let Some(config_file) = given_args.next() else {
        return Err("Missing config file!".into());
    };

    if let Some(_rest) = given_args.next() {
        // TODO: use logging?
        println!("NOTE: explicit binary passing not implementated (only config is allowed)");
    };

    Ok(CliOptions { config_file })
}

/// Entry point: parse args, load the config file, then hand off to the wrapper runtime.
fn main() -> anyhow::Result<()> {
    let config_file = match parse_args() {
        Ok(opts) => opts.config_file,
        Err(err) => {
            show_usage();
            anyhow::bail!("{err}");
        }
    };

    let config_path = Path::new(&config_file);
    if !config_path.exists() {
        anyhow::bail!("config file '{config_file}' does not exist");
    }

    let _config_content = std::fs::read_to_string(config_path)
        .with_context(|| format!("failed to read config file '{config_file}'"))?;

    Ok(())
}
