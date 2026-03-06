use clap::Parser;
use omninova_core::cli::{Cli, Commands, DaemonCommands, run_cli};

#[derive(Debug, serde::Deserialize)]
struct DaemonCheckResult {
    ok: bool,
}

#[tokio::main]
async fn main() {
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
