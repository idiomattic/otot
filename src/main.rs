use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
struct Cli {
    address: String,
    #[command(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,
}

fn main() -> Result<()> {
    let args = Cli::parse();

    env_logger::Builder::new()
        .filter_level(args.verbosity.into())
        .init();

    if args.address.is_empty() {
        anyhow::bail!("provided address must be a non-empty string");
    }

    println!("Hello, world!");

    Ok(())
}
