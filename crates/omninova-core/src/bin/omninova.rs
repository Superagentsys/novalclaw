use clap::Parser;
use omninova_core::cli::{Cli, Commands, DaemonCommands, run_cli};

#[derive(Debug, serde::Deserialize)]
struct DaemonCheckResult {
    ok: bool,
}

fn main() {
    // On Windows, the default main-thread stack (1 MB) is too small for the
    // deeply-nested async futures produced by the agent pipeline in debug builds.
    // Spin up a tokio runtime on a thread with an 8 MB stack to avoid overflow.
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime")
                .block_on(async_main());
        })
        .expect("failed to spawn main thread")
        .join()
        .expect("main thread panicked");
}

async fn async_main() {
    let cli = Cli::parse();
    let is_daemon_check = matches!(
        &cli.command,
        Commands::Daemon {
            command: DaemonCommands::Check { .. }
        }
    );
    match run_cli(cli).await {
        Ok(output) => {
            println!("{output}");
            if is_daemon_check {
                match serde_json::from_str::<DaemonCheckResult>(&output) {
                    Ok(result) if !result.ok => {
                        std::process::exit(2);
                    }
                    _ => {}
                }
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
