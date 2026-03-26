use anyhow::Result;
use colored::Colorize;

use crate::api::Client;
use crate::OutputFormat;

pub async fn execute(client: &Client, format: OutputFormat) -> Result<()> {
    let status = client.get_status().await?;
    
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&status)?);
        }
        OutputFormat::Text => {
            println!("{}", "OmniNova Claw Status".bold().underline());
            println!("  Status:  {}", 
                if status.status == "ok" { 
                    "✓ Running".green() 
                } else { 
                    "✗ Error".red() 
                }
            );
            println!("  Version: {}", status.version.cyan());
            println!("  Uptime:  {}s", status.uptime.to_string().cyan());
        }
    }

    Ok(())
}
