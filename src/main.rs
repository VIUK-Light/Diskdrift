use diskdrift::cli::args::{self, Command};
use diskdrift::cli::commands;
use diskdrift::core::interrupt;

fn main() {
    interrupt::install();

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = match args::parse(&argv) {
        Ok(Command::Help(_)) => {
            print!("{}", args::usage());
            commands::EXIT_OK
        }
        Ok(cmd) => match commands::run(cmd) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("error: {e}");
                commands::EXIT_ERROR
            }
        },
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!();
            eprintln!("Run `diskdrift help` for usage.");
            2
        }
    };
    std::process::exit(code);
}
