use anyhow::Result;
use colored::Colorize;
use tabled::{Table, Tabled};
use tabled::settings::Style;

use crate::api::Client;
use crate::commands::AgentsCommands;
use crate::commands::agent_advanced::{self, ExportFormat, BatchOperation};
use crate::OutputFormat;

#[derive(Tabled)]
struct AgentRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "MBTI")]
    mbti: String,
    #[tabled(rename = "Status")]
    status: String,
}

pub async fn execute(cmd: AgentsCommands, client: &Client, format: OutputFormat) -> Result<()> {
    match cmd {
        AgentsCommands::List => {
            let response = client.list_agents().await?;
            
            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                }
                OutputFormat::Text => {
                    if response.agents.is_empty() {
                        println!("{}", "No agents found.".yellow());
                        return Ok(());
                    }

                    let rows: Vec<AgentRow> = response.agents
                        .into_iter()
                        .map(|a| AgentRow {
                            id: a.id[..8].to_string(),
                            name: a.name,
                            mbti: a.mbti_type.unwrap_or_else(|| "-".to_string()),
                            status: if a.enabled {
                                "● enabled".green().to_string()
                            } else {
                                "○ disabled".dimmed().to_string()
                            },
                        })
                        .collect();

                    let table = Table::new(rows)
                        .with(Style::modern_rounded())
                        .to_string();
                    
                    println!("{}", table);
                    println!("\nTotal: {} agents", response.total);
                }
            }
        }
        AgentsCommands::Show { id } => {
            let agent = client.get_agent(&id).await?;
            
            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&agent)?);
                }
                OutputFormat::Text => {
                    println!("{}", "Agent Details".bold().underline());
                    println!("  ID:          {}", agent.id.cyan());
                    println!("  Name:        {}", agent.name.bold());
                    println!("  Description: {}", agent.description.unwrap_or_else(|| "-".to_string()));
                    println!("  MBTI Type:   {}", agent.mbti_type.unwrap_or_else(|| "-".to_string()));
                    println!("  Provider:    {}", agent.provider.unwrap_or_else(|| "-".to_string()));
                    println!("  Model:       {}", agent.model.unwrap_or_else(|| "-".to_string()));
                    println!("  Status:      {}", 
                        if agent.enabled { 
                            "enabled".green() 
                        } else { 
                            "disabled".red() 
                        }
                    );
                    println!("  Created:     {}", agent.created_at.dimmed());
                    println!("  Updated:     {}", agent.updated_at.dimmed());
                }
            }
        }
        AgentsCommands::Create { name, mbti } => {
            let agent = client.create_agent(&name, mbti.as_deref()).await?;
            
            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&agent)?);
                }
                OutputFormat::Text => {
                    println!("{} Agent created successfully!", "✓".green());
                    println!("  ID:   {}", agent.id.cyan());
                    println!("  Name: {}", agent.name.bold());
                }
            }
        }
        AgentsCommands::Delete { id, force } => {
            if !force {
                print!("Are you sure you want to delete agent {}? [y/N] ", id.cyan());
                use std::io::Write;
                std::io::stdout().flush()?;
                
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Deletion cancelled.");
                    return Ok(());
                }
            }

            client.delete_agent(&id).await?;
            
            match format {
                OutputFormat::Json => {
                    println!("{{\"success\": true, \"message\": \"Agent deleted\"}}");
                }
                OutputFormat::Text => {
                    println!("{} Agent {} deleted successfully", "✓".green(), id.cyan());
                }
            }
        }
        AgentsCommands::Export { output, format: fmt, ids } => {
            let export_format = match fmt.as_str() {
                "yaml" | "yml" => ExportFormat::Yaml,
                _ => ExportFormat::Json,
            };
            agent_advanced::export_agents(client, ids, &output, export_format).await?;
        }
        AgentsCommands::Import { input, skip_existing } => {
            agent_advanced::import_agents(client, &input, skip_existing).await?;
        }
        AgentsCommands::Batch { operation, ids } => {
            let op = match operation.as_str() {
                "enable" => BatchOperation::Enable,
                "disable" => BatchOperation::Disable,
                "delete" => BatchOperation::Delete,
                _ => {
                    eprintln!("{} Unknown operation: {}", "Error:".red(), operation);
                    return Ok(());
                }
            };
            agent_advanced::batch_operation(client, op, ids).await?;
        }
        AgentsCommands::Stats { id } => {
            agent_advanced::show_stats(client, id, format).await?;
        }
        AgentsCommands::History { agent, limit, detailed } => {
            agent_advanced::show_history(client, agent.as_deref(), limit, detailed, format).await?;
        }
    }

    Ok(())
}
