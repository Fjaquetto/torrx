use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = torrx::cli::Cli::parse();

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("error: cannot start async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(torrx::run(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
