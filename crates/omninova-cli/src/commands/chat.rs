use anyhow::Result;
use colored::Colorize;

use crate::api::Client;
use crate::OutputFormat;

pub async fn execute(agent_id: &str, message: &str, client: &Client, format: OutputFormat) -> Result<()> {
    let response = client.chat(agent_id, message).await?;
    
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
        OutputFormat::Text => {
            println!("{} {}", "Agent:".bold().cyan(), response.agent_id);
            println!("{}", "─".repeat(50).dimmed());
            println!("{}", response.message);
        }
    }

    Ok(())
}
