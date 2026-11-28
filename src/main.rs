
use std::path::Path;
use std::process::ExitCode;

use config::life::{Domain, Life};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("check") => {
            let path = args.get(2).map_or("data/life.yaml", String::as_str);
            check(Path::new(path))
        }
        _ => {
            eprintln!("lifeplan {}\nusage: lifeplan check [life.yaml]", env!("CARGO_PKG_VERSION"));
            ExitCode::from(2)
        }
    }
}

fn check(path: &Path) -> ExitCode {
    match Life::load(path) {
        Ok(loaded) => {
            println!(
                "ok: {} domains, {} arcs, {} people",
                config::life::Domain::ALL.len(),
                loaded.life.arcs.len(),
                loaded.people.len()
            );
            if !loaded.tbds.is_empty() {
                println!("{} settings still TBD", loaded.tbds.len());
                for t in &loaded.tbds {
                    println!(" - {t}");
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

