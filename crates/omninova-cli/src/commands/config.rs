use anyhow::Result;
use colored::Colorize;

use crate::config::Config;
use crate::commands::ConfigCommands;

pub async fn execute(cmd: ConfigCommands, config: &Config) -> Result<()> {
    match cmd {
        ConfigCommands::Show => {
            println!("{}", "Current Configuration".bold().underline());
            println!("  Server URL:     {}", config.server_url.cyan());
            println!("  Default Agent:  {}", 
                config.default_agent.as_deref().unwrap_or("-").cyan()
            );
            println!("  Output Format:  {}", config.output_format.cyan());
            println!();
            println!("Config file location: {}", 
                Config::config_path()?.display().to_string().dimmed()
            );
        }
        ConfigCommands::Set { key, value } => {
            let mut new_config = config.clone();
            
            match key.as_str() {
                "server_url" => {
                    new_config.server_url = value;
                    println!("{} Server URL updated", "✓".green());
                }
                "default_agent" => {
                    new_config.default_agent = Some(value);
                    println!("{} Default agent updated", "✓".green());
                }
                "output_format" => {
                    new_config.output_format = value;
                    println!("{} Output format updated", "✓".green());
                }
                _ => {
                    anyhow::bail!("Unknown config key: {}. Valid keys: server_url, default_agent, output_format", key);
                }
            }
            
            new_config.save(None)?;
        }
        ConfigCommands::Init => {
            let default_config = Config::default();
            default_config.save(None)?;
            println!("{} Configuration file created at: {}", 
                "✓".green(),
                Config::config_path()?.display().to_string().cyan()
            );
        }
    }

    Ok(())
}
