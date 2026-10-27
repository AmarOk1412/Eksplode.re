use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use eksplode_re_cli::app;
use eksplode_re_cli::args::{self, Command, USAGE};

fn main() -> ExitCode {
    match args::parse(std::env::args().skip(1)) {
        Ok(Command::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("eksplode-re-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::Host(opts)) => finish(app::run_host(&opts)),
        Ok(Command::Join(opts)) => finish(app::run_join(&opts)),
        Ok(Command::Run(opts)) => {
            let seed = opts.seed.unwrap_or_else(time_seed);
            finish(app::run(&opts, seed))
        }
        Err(e) => {
            eprintln!("error: {e}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn time_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn finish(result: Result<(), app::AppError>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
