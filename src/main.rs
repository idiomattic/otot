use anyhow::Result;
use clap::Parser;
use url::Url;

#[derive(Parser)]
struct Cli {
    address: String,
    #[command(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,
}

#[derive(Debug)]
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
        // XXX: for now, we're assuming that, if the user didn't input a scheme, we can differentiate between a fuzzy pattern
        //   and a domain that just needs https prepended by the presence of a '.'
        if url.host_str().map_or(false, |h| h.contains('.')) {
            return InputType::FullUrl(url);
        }
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

    let parsed = classify_input(&args.address);

    println!("parsed: {:?}", &parsed);

    Ok(())
}
