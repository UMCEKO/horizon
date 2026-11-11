//! `sides`: prints the client/server census of `impl Game`; it reads the source and writes nothing.
use std::path::PathBuf;

use horizon_game_discovery::census::{self, Listing};

const USAGE: &str = "usage: sides [--json] [--list methods|fields|files] [--root DIR]";

enum Output {
    Tables,
    Json,
    List(Listing),
}

pub fn run(options: &[String]) -> Result<(), String> {
    let mut output = Output::Tables;
    let mut root = None;
    let mut options = options.iter();
    while let Some(option) = options.next() {
        match option.as_str() {
            "--json" => output = Output::Json,
            "--list" => {
                output = Output::List(match options.next().map(String::as_str) {
                    Some("methods") => Listing::Methods,
                    Some("fields") => Listing::Fields,
                    Some("files") => Listing::Files,
                    _ => return Err(USAGE.into()),
                });
            }
            "--root" => root = Some(PathBuf::from(options.next().ok_or(USAGE)?)),
            _ => return Err(USAGE.into()),
        }
    }
    let root = match root {
        Some(root) => root,
        None => super::repo_root()?,
    };
    let census = census::run(&root)?;
    match output {
        Output::Tables => print!("{}", census.text()),
        Output::Json => println!("{}", census.json()?),
        Output::List(what) => print!("{}", census.listing(what)),
    }
    Ok(())
}
