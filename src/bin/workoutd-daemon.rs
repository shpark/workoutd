use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use workoutd::{default_db_path, health_check, open_database};

#[derive(Parser)]
#[command(name = "workoutd-daemon")]
#[command(about = "Lightweight workoutd database health service")]
struct Cli {
    #[arg(long, env = "WORKOUTD_DB")]
    db: Option<PathBuf>,
    #[arg(long)]
    once: bool,
    #[arg(long, default_value_t = 60)]
    interval_seconds: u64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = cli.db.clone().map(Ok).unwrap_or_else(default_db_path)?;

    loop {
        let conn = open_database(&db_path)?;
        health_check(&conn)?;
        eprintln!("workoutd-daemon healthy db={}", db_path.display());

        if cli.once {
            return Ok(());
        }

        thread::sleep(Duration::from_secs(cli.interval_seconds.max(1)));
    }
}
