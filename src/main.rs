use std::process;

use letterlift::{config, stats, ui};

fn main() {
    let config = config::Config::parse_args();
    if let Err(e) = config.validate() {
        eprintln!("Error: {}", e);
        process::exit(2);
    }

    let mut stats_manager = stats::StatsManager::new();

    if config.stats {
        for line in stats_manager.stats().lines() {
            println!("{}", line);
        }
        if let Some(path) = stats_manager.file_path() {
            println!("\nStats file: {}", path.display());
        }
        return;
    }

    if config.reset_stats {
        if let Err(e) = stats_manager.reset() {
            eprintln!("Error resetting statistics: {}", e);
            process::exit(1);
        }
        println!("Statistics reset.");
        return;
    }

    match ui::run(&config, &mut stats_manager) {
        Ok(Some(share)) => println!("{}", share),
        Ok(None) => {}
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}
