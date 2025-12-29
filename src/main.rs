use anyhow::Result;
use clap::Parser;
use url::Url;

#[derive(Parser)]
struct Cli {
    address: String,
    #[command(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,
}

enum InputType {
    FullUrl(Url),
    FuzzyPattern(Vec<String>),
}

fn classify_input(address: &str) -> InputType {
    if let Ok(url) = Url::parse(address) {
        return InputType::FullUrl(url);
    }

    let with_scheme = format!("https://{}", address);
    if let Ok(url) = Url::parse(&with_scheme) {
        return InputType::FullUrl(url);
    }

    InputType::FuzzyPattern(address.split('/').map(String::from).collect())
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
