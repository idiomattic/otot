# otot
A command-line tool for quickly opening URLs with fuzzy matching and frecency-based search.

[![CI](https://github.com/idiomattic/otot/workflows/CI/badge.svg)](https://github.com/idiomattic/otot/actions)

## Overview
`otot` ("Open Tab Over There") helps you quickly open links from your terminal by remembering each usage and allowing fuzzy pattern matching. Instead of typing full URLs, use partial matches to quickly access your most frequently and recently visited sites.

### Installation

Via Homebrew (macOS/Linux)
```bash
brew tap idiomattic/otot
brew install otot
```
Via Cargo
```bash
cargo install otot
```
Or build from source:
```bash
git clone https://github.com/idiomattic/otot
cd otot
cargo install --path .
```

> ### Tip
>
> Aliasing the `open` subcommand is helpful for ergonomic, quick usage:
>```bash
> alias o="otot open"
>```

## Usage
### Open a full URL
```bash
otot open github.com/rust-lang/rust
```
The tool automatically opens the URL in your default browser.

### Fuzzy matching
```bash
otot open github/rust
```
This finds the most relevant URL containing both "github" and "rust" based on your visit history. The ranking uses a frecency algorithm that considers both frequency (how often you visit) and recency (when you last visited).

#### Query the database
```bash
otot query github/rust
```
This returns all matches in a table, using the same query as `open`, for debugging.

### Configuration
Set your preferred browser:
```bash
otot config set -k preferred_browser -n firefox
```
View current settings:
```bash
otot config get -k preferred_browser
```
Show config file location:
```bash
otot config path
```

## How It Works
`otot` maintains a local SQLite database tracking your URL visits. When you use fuzzy matching, it:
1. Breaks your input into segments
2. Searches for URLs in your history that match on:
    - fuzzy match of base domain (e.g. "github.com")
    - fuzzy match on last path segment (e.g. "rust")
    - intermediate segments may be skipped, but when provided, must fuzzy match in the correct *relative order* (not all must be provided)
3. Ranks results by frecency score (visit count × recency multiplier)
4. Opens the best match

### Configuration
Default config location: `~/.config/otot/default-config.toml`
#### Available settings
- `preferred_browser`: Browser command (e.g., "firefox", "chrome", "brave")

#### Database
The database is a simple SQLite file that tracks:
- URLs you've opened
- Visit counts
- Last access timestamps
- URL segments for fuzzy matching

Location:
| Platform |                 Value                |                  Example                 |
|:--------:|:------------------------------------:|:----------------------------------------:|
| Linux    | $XDG_DATA_HOME or $HOME/.local/share | /home/alice/.local/share                 |
| macOS    | $HOME/Library/Application Support    | /Users/Alice/Library/Application Support |
| Windows  | {FOLDERID_LocalAppData}              | C:\Users\Alice\AppData\Local             |

## Privacy
The database stores visit counts and timestamps but no personal browsing data beyond the URLs you explicitly open with `otot`.

## Inspiration
This project is heavily inspired by [zoxide](https://github.com/ajeetdsouza/zoxide), a wonderful CLI for navigating between directories.

## License
MIT
