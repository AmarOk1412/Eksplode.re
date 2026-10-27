use std::process::ExitCode;
use std::sync::mpsc;

use eksplode_re_web::args::{parse, Command, USAGE};
use eksplode_re_web::origin::warnings;
use eksplode_re_web::Server;

fn main() -> ExitCode {
    match parse(std::env::args().skip(1)) {
        Ok(Command::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("eksplode-re-server {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::Run(opts)) => {
            for line in warnings(&opts) {
                eprintln!("{line}");
            }
            let server = match Server::start(opts) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::from(1);
                }
            };
            print!("{}", server.banner());

            let (tx, rx) = mpsc::channel();
            if let Err(e) = ctrlc::set_handler(move || {
                let _ = tx.send(());
            }) {
                eprintln!("error: cannot install the Ctrl+C handler: {e}");
                return ExitCode::from(1);
            }
            let _ = rx.recv();
            println!("Stopping...");
            server.shutdown();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
