use std::io;
use std::process::ExitCode;
use std::sync::mpsc;
use std::sync::Arc;

use eksplode_re_net::events::{EventSink, JsonlEventWriter};
use eksplode_re_net::host::HostServices;
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
            // The web service has no TUI to protect, so stdout is always the JSONL event log;
            // human-readable banners and diagnostics go to stderr instead (T069).
            let writer = JsonlEventWriter::start(io::stdout());
            let events: Arc<dyn EventSink> = writer.clone();
            let server = match Server::start_with_services(
                opts,
                HostServices::default().with_events(events),
            ) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {e}");
                    writer.shutdown();
                    return ExitCode::from(1);
                }
            };
            eprint!("{}", server.banner());

            let (tx, rx) = mpsc::channel();
            if let Err(e) = ctrlc::set_handler(move || {
                let _ = tx.send(());
            }) {
                eprintln!("error: cannot install the Ctrl+C handler: {e}");
                writer.shutdown();
                return ExitCode::from(1);
            }
            let _ = rx.recv();
            eprintln!("Stopping...");
            server.shutdown();
            writer.shutdown();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
