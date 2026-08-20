//! Claude Code-style default CLI: streaming REPL and `-p` one-shot chat.

use crate::channels::adapters::cli::inbound_from_cli;
use crate::config::Config;
use crate::cron::CronStore;
use crate::gateway::GatewayRuntime;
use crate::skills::{list_command_palette, parse_slash_command, ParsedSlashCommand};
use anyhow::Result;
use serde_json::Value;
use std::io::{self, IsTerminal, Read, Write};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

const REPL_HELP: &str = "\
OmniNova CLI  (Claude Code style)

  Type a message to talk to the agent. Slash commands:

    /help       Show this help
    /exit       Quit  (also /quit or Ctrl-D)
    /sessions   List stored sessions
    /skills     List installed skill catalog
    /skill <slug> [prompt]  Activate one skill and optionally chat
    /cron       List automation jobs
    /kb         List knowledge-base documents
    /models     List configured providers
    /config     Show config path and defaults
    /tui        Hint for the fullscreen TUI
    /clear      Clear the screen
";

pub async fn run_repl(config: Config, session_id: Option<String>) -> Result<String> {
    let session = session_id.unwrap_or_else(|| "omninova-cli".to_string());
    let runtime = GatewayRuntime::new(config.clone());
    println!("OmniNova  ·  session={session}  ·  /help for commands");
    println!("Workspace {}", config.workspace_dir.display());

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        print!("> ");
        io::stdout().flush()?;
        let Some(line) = lines.next_line().await? else {
            println!();
            break;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(crate::skills::ParsedSlashCommand::Skill { id, rest }) =
            crate::skills::parse_slash_command(trimmed)
        {
            if !matches!(
                trimmed.split_whitespace().next().unwrap_or(""),
                "/exit" | "/quit" | "/q" | "/help" | "/?" | "/clear" | "/tui" | "/config"
                    | "/sessions" | "/skills" | "/cron" | "/kb" | "/knowledge" | "/models"
            ) {
                if rest.is_empty() {
                    let palette = crate::skills::list_command_palette(&config);
                    if let Some(item) = palette.skills.iter().find(|item| item.id == id) {
                        println!(
                            "skill {} ready. Send a prompt with /skill {} <text>",
                            item.display_name, item.command_alias
                        );
                    } else {
                        println!("skill unavailable: {id}");
                    }
                    continue;
                }
                stream_prompt_with_skill(&runtime, &rest, Some(session.clone()), &id).await?;
                continue;
            }
        }
        if let Some(output) = handle_slash(&runtime, &config, trimmed).await? {
            if output == "__exit__" {
                break;
            }
            if !output.is_empty() {
                println!("{output}");
            }
            continue;
        }
        stream_prompt(&runtime, trimmed, Some(session.clone())).await?;
    }
    Ok(String::new())
}

pub async fn run_oneshot(
    config: Config,
    prompt: String,
    session_id: Option<String>,
) -> Result<String> {
    let runtime = GatewayRuntime::new(config);
    stream_prompt(&runtime, &prompt, session_id).await
}

pub async fn run_default(config: Config, prompt: Option<String>, session_id: Option<String>) -> Result<String> {
    if let Some(text) = prompt.filter(|s| !s.trim().is_empty()) {
        return run_oneshot(config, text, session_id).await;
    }
    if io::stdin().is_terminal() {
        return run_repl(config, session_id).await;
    }
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    let text = buf.trim().to_string();
    if text.is_empty() {
        anyhow::bail!("no prompt given; pass -p/--prompt or run in a terminal");
    }
    run_oneshot(config, text, session_id).await
}

async fn stream_prompt(
    runtime: &GatewayRuntime,
    prompt: &str,
    session_id: Option<String>,
) -> Result<String> {
    stream_prompt_with_skill(runtime, prompt, session_id, "").await
}

async fn stream_prompt_with_skill(
    runtime: &GatewayRuntime,
    prompt: &str,
    session_id: Option<String>,
    skill_id: &str,
) -> Result<String> {
    let mut inbound = inbound_from_cli(prompt.to_string(), session_id, None);
    if !skill_id.trim().is_empty() {
        inbound.metadata.insert(
            "skill_invocations".to_string(),
            serde_json::json!([{
                "skillId": skill_id,
                "source": "slash_command"
            }]),
        );
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
    let printer = tokio::spawn(async move {
        let mut printed_delta = false;
        while let Some(event) = rx.recv().await {
            let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
            match kind {
                "model_delta" => {
                    if let Some(content) = event.get("content").and_then(Value::as_str) {
                        print!("{content}");
                        let _ = io::stdout().flush();
                        printed_delta = true;
                    }
                }
                "tool_started" | "tool_call_created" => {
                    if printed_delta {
                        println!();
                        printed_delta = false;
                    }
                    let title = event
                        .get("title")
                        .or_else(|| event.get("tool_name"))
                        .and_then(Value::as_str)
                        .unwrap_or("tool");
                    println!("⚙ {title}");
                }
                "run_failed" | "error" => {
                    if printed_delta {
                        println!();
                    }
                    let err = event
                        .get("error")
                        .or_else(|| event.get("message"))
                        .and_then(Value::as_str)
                        .unwrap_or("run failed");
                    eprintln!("error: {err}");
                }
                "run_cancelled" => {
                    if printed_delta {
                        println!();
                    }
                    eprintln!("cancelled");
                }
                _ => {}
            }
        }
        printed_delta
    });
    let resp = runtime.process_inbound_streaming(&inbound, tx).await?;
    let printed_delta = printer.await.unwrap_or(false);
    if printed_delta {
        println!();
    } else if !resp.reply.trim().is_empty() {
        println!("{}", resp.reply.trim_end());
    }
    Ok(resp.reply)
}

async fn handle_slash(
    runtime: &GatewayRuntime,
    config: &Config,
    line: &str,
) -> Result<Option<String>> {
    let cmd = line.split_whitespace().next().unwrap_or("");
    match cmd {
        "/exit" | "/quit" | "/q" => Ok(Some("__exit__".into())),
        "/help" | "/?" => Ok(Some(REPL_HELP.trim_end().into())),
        "/clear" => {
            print!("\x1b[2J\x1b[H");
            Ok(Some(String::new()))
        }
        "/tui" => Ok(Some("Fullscreen TUI: run `omninova tui`.".into())),
        "/config" => Ok(Some(format!(
            "config: {}\nworkspace: {}\nprovider: {}\nmodel: {}\ngateway: http://{}:{}/app",
            config.config_path.display(),
            config.workspace_dir.display(),
            config.default_provider.as_deref().unwrap_or("-"),
            config.default_model.as_deref().unwrap_or("-"),
            if config.gateway.host == "0.0.0.0" {
                "127.0.0.1"
            } else {
                &config.gateway.host
            },
            config.gateway.port
        ))),
        "/sessions" => {
            let snapshot = runtime.session_tree_snapshot().await?;
            Ok(Some(serde_json::to_string_pretty(&snapshot)?))
        }
        "/skills" => {
            let palette = crate::skills::list_command_palette(config);
            Ok(Some(serde_json::to_string_pretty(&serde_json::json!({
                "generation": palette.generation,
                "openSkillsEnabled": palette.open_skills_enabled,
                "skills": palette.skills,
                "empty": palette.skills_empty_reason,
            }))?))
        }
        "/skill" => {
            let parsed = parse_slash_command(line);
            match parsed {
                Some(ParsedSlashCommand::Skill { id, rest }) if rest.is_empty() => {
                    let palette = list_command_palette(config);
                    let found = palette.skills.iter().find(|item| item.id == id);
                    Ok(Some(serde_json::to_string_pretty(&serde_json::json!({
                        "skillId": id,
                        "available": found.is_some(),
                        "item": found,
                    }))?))
                }
                _ => Ok(Some("usage: /skill <slug> [prompt]".into())),
            }
        }
        "/cron" => {
            let store = CronStore::open(config.workspace_dir.join("cron.json")).await?;
            Ok(Some(serde_json::to_string_pretty(&store.list().await)?))
        }
        "/kb" | "/knowledge" => {
            let store = crate::knowledge::KnowledgeStore::open_in(&config.workspace_dir).await?;
            Ok(Some(serde_json::to_string_pretty(&store.list(None).await)?))
        }
        "/models" => Ok(Some(serde_json::to_string_pretty(&models_view(config))?)),
        _ if cmd.starts_with('/') => Ok(Some(format!(
            "unknown command {cmd}. Type /help."
        ))),
        _ => Ok(None),
    }
}

pub fn models_view(config: &Config) -> Value {
    let providers: Vec<Value> = config
        .model_providers
        .iter()
        .map(|(id, p)| {
            serde_json::json!({
                "id": id,
                "enabled": p.enabled,
                "default_model": p.default_model,
                "models": p.models,
                "base_url": p.base_url,
            })
        })
        .collect();
    let legacy: Vec<Value> = config
        .providers
        .iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id,
                "name": p.name,
                "type": p.provider_type,
                "enabled": p.enabled,
                "models": p.models,
            })
        })
        .collect();
    serde_json::json!({
        "default_provider": config.default_provider,
        "default_model": config.default_model,
        "model_providers": providers,
        "providers": legacy,
    })
}
