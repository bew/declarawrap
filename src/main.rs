mod config;

use std::path::Path;

static PROGRAM_NAME: &str = "declarawrap";

fn show_usage() {
    // println!("Usage: declarawrap <config-file> [<bin> [<args>...]]");
    println!("Usage: {PROGRAM_NAME} <config-file>");
}

#[derive(Debug)]
struct CliOptions {
  pub config_file: String,
}

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

fn main() -> anyhow::Result<()> {
    let config_file = match parse_args() {
        Ok(opts) => opts.config_file,
        Err(err) => {
            eprintln!("ERROR: {err}");
            show_usage();
            return Ok(())
        }
    };

    let config_path = Path::new(&config_file);
    if !config_path.exists() {
        println!("ERROR: Config file {config_file} does not exist!");
    }

    let config_content = std::fs::read_to_string(config_path).unwrap();
    // let body = hcl::parse(&config_content).unwrap();
    // println!("Config HCL body: {body:#?}");

    // let config: config::BinWrapperConfig = hcl::de::from_str(&config_content)?;
    // println!("Config: {config:#?}");

    Ok(())
}
