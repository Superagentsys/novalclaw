use anyhow::{Result, Context};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::fs;

use crate::api::Client;
use crate::OutputFormat;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExport {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub mbti_type: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsExport {
    pub version: String,
    pub exported_at: String,
    pub agents: Vec<AgentExport>,
}

/// Export agents to file
pub async fn export_agents(
    client: &Client,
    agent_ids: Option<Vec<String>>,
    output: &str,
    format: ExportFormat,
) -> Result<()> {
    let agents = if let Some(ids) = agent_ids {
        // Export specific agents
        let mut result = Vec::new();
        for id in ids {
            match client.get_agent(&id).await {
                Ok(agent) => result.push(agent),
                Err(e) => eprintln!("{} Failed to get agent {}: {}", "Warning:".yellow(), id, e),
            }
        }
        result
    } else {
        // Export all agents
        client.list_agents().await?.agents
    };

    let export = AgentsExport {
        version: "1.0".to_string(),
        exported_at: chrono::Local::now().to_rfc3339(),
        agents: agents.into_iter().map(|a| AgentExport {
            id: a.id,
            name: a.name,
            description: a.description,
            mbti_type: a.mbti_type,
            provider: a.provider,
            model: a.model,
            system_prompt: None, // TODO: fetch from config
            temperature: None,
            max_tokens: None,
            enabled: a.enabled,
        }).collect(),
    };

    let content = match format {
        ExportFormat::Json => serde_json::to_string_pretty(&export)?,
        ExportFormat::Yaml => serde_yaml::to_string(&export)?,
    };

    fs::write(output, content)
        .with_context(|| format!("Failed to write export file: {}", output))?;

    println!("{} Exported {} agents to {}", "✓".green(), export.agents.len(), output);
    Ok(())
}

/// Import agents from file
pub async fn import_agents(
    client: &Client,
    input: &str,
    skip_existing: bool,
) -> Result<()> {
    let content = fs::read_to_string(input)
        .with_context(|| format!("Failed to read import file: {}", input))?;

    let export: AgentsExport = if input.ends_with(".yaml") || input.ends_with(".yml") {
        serde_yaml::from_str(&content)?
    } else {
        serde_json::from_str(&content)?
    };

    let mut imported = 0;
    let mut skipped = 0;
    let mut failed = 0;

    for agent in export.agents {
        // Check if agent already exists
        if skip_existing {
            if let Ok(_) = client.get_agent(&agent.id).await {
                println!("{} Skipping existing agent: {}", "→".blue(), agent.name);
                skipped += 1;
                continue;
            }
        }

        match client.create_agent(&agent.name, agent.mbti_type.as_deref()).await {
            Ok(_) => {
                println!("{} Imported agent: {}", "✓".green(), agent.name);
                imported += 1;
            }
            Err(e) => {
                eprintln!("{} Failed to import agent {}: {}", "✗".red(), agent.name, e);
                failed += 1;
            }
        }
    }

    println!("\nImport complete: {} imported, {} skipped, {} failed", 
        imported.to_string().green(),
        skipped.to_string().blue(),
        failed.to_string().red()
    );
    Ok(())
}

/// Batch operations on agents
pub async fn batch_operation(
    client: &Client,
    operation: BatchOperation,
    agent_ids: Vec<String>,
) -> Result<()> {
    let mut success = 0;
    let mut failed = 0;

    for id in &agent_ids {
        let result = match &operation {
            BatchOperation::Enable => client.enable_agent(id).await,
            BatchOperation::Disable => client.disable_agent(id).await,
            BatchOperation::Delete => client.delete_agent(id).await,
        };

        match result {
            Ok(_) => {
                println!("{} {} agent: {}", 
                    "✓".green(),
                    match operation {
                        BatchOperation::Enable => "Enabled",
                        BatchOperation::Disable => "Disabled",
                        BatchOperation::Delete => "Deleted",
                    },
                    id
                );
                success += 1;
            }
            Err(e) => {
                eprintln!("{} Failed to {} agent {}: {}", 
                    "✗".red(),
                    match operation {
                        BatchOperation::Enable => "enable",
                        BatchOperation::Disable => "disable",
                        BatchOperation::Delete => "delete",
                    },
                    id,
                    e
                );
                failed += 1;
            }
        }
    }

    println!("\nBatch operation complete: {} succeeded, {} failed", 
        success.to_string().green(),
        failed.to_string().red()
    );
    Ok(())
}

/// Show agent statistics
pub async fn show_stats(
    client: &Client,
    agent_id: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let stats = if let Some(id) = agent_id {
        vec![client.get_agent_stats(&id).await?]
    } else {
        client.get_all_agent_stats().await?
    };

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        OutputFormat::Text => {
            println!("{}", "Agent Statistics".bold().underline());
            println!();
            
            for stat in stats {
                println!("{} {}", "Agent:".bold(), stat.agent_name);
                println!("  {} {}", "Messages:".dimmed(), stat.message_count);
                println!("  {} {} ms", "Avg Response:".dimmed(), stat.avg_response_time_ms);
                println!("  {} {}", "Sessions:".dimmed(), stat.session_count);
                println!("  {} {}", "Last Active:".dimmed(), stat.last_active);
                println!();
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    Json,
    Yaml,
}

#[derive(Debug, Clone, Copy)]
pub enum BatchOperation {
    Enable,
    Disable,
    Delete,
}


