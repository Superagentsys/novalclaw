//! Interactive terminal UI for OmniNova (Claude Code-style), built on ratatui.
//!
//! A single in-process `Agent` is kept for the whole run (multi-turn context
//! lives in memory). Each turn streams token deltas and tool steps live into
//! the transcript via `Agent::process_message_streaming`.

use crate::agent::{Agent, AgentEvent};
use crate::config::Config;
use crate::gateway::GatewayRuntime;
use anyhow::{Context, Result};
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};
use std::io::{stdout, Stdout};
use std::time::Duration;
use tokio::sync::mpsc;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Clone, Copy, PartialEq)]
enum Role {
    User,
    Assistant,
    System,
    Error,
    Tool,
}

struct Msg {
    role: Role,
    text: String,
}

struct App {
    provider: String,
    model: String,
    transcript: Vec<Msg>,
    input: Vec<char>,
    cursor: usize,
    busy: bool,
    spinner: usize,
    /// Index of the assistant message currently being streamed into.
    streaming_idx: Option<usize>,
    /// Lines scrolled up from the bottom (0 = stuck to bottom).
    scroll_back: usize,
    show_steps: bool,
    should_quit: bool,
}

impl App {
    fn new(config: &Config) -> Self {
        let provider = config
            .default_provider
            .clone()
            .unwrap_or_else(|| "(default)".to_string());
        let model = config
            .default_model
            .clone()
            .unwrap_or_else(|| "(auto)".to_string());
        let mut app = Self {
            provider,
            model,
            transcript: Vec::new(),
            input: Vec::new(),
            cursor: 0,
            busy: false,
            spinner: 0,
            streaming_idx: None,
            scroll_back: 0,
            show_steps: true,
            should_quit: false,
        };
        app.transcript.push(Msg {
            role: Role::System,
            text: "欢迎使用 OmniNova 终端（流式）。回车发送；Esc / Ctrl+C 退出；\
                   PageUp/PageDown 滚动；Ctrl+L 清屏；Ctrl+T 切换工具步骤显示。"
                .to_string(),
        });
        app
    }

    fn input_string(&self) -> String {
        self.input.iter().collect()
    }
}

/// Restores the terminal on drop, even if the UI loop panics or errors out.
struct TermGuard;
impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}

pub async fn run_tui(config: Config) -> Result<String> {
    enable_raw_mode().context("无法进入终端 raw 模式（请在交互式终端中运行 omninova tui）")?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen).context("无法进入备用屏幕")?;
    let _guard = TermGuard;

    let backend = CrosstermBackend::new(out);
    let mut terminal: Terminal<CrosstermBackend<Stdout>> =
        Terminal::new(backend).context("无法初始化终端后端")?;

    let runtime = GatewayRuntime::new(config.clone());
    let mut app = App::new(&config);

    // A single long-lived agent owns the conversation; a worker task drives it
    // sequentially so the UI loop never blocks on model calls.
    let agent: Agent = runtime.build_interactive_agent().await?;
    let (prompt_tx, mut prompt_rx) = mpsc::unbounded_channel::<String>();
    let (evt_tx, mut evt_rx) = mpsc::unbounded_channel::<AgentEvent>();
    tokio::spawn(async move {
        let mut agent = agent;
        while let Some(prompt) = prompt_rx.recv().await {
            if let Err(e) = agent.process_message_streaming(&prompt, &evt_tx).await {
                let _ = evt_tx.send(AgentEvent::Error(format!("请求失败：{e}")));
            }
        }
    });

    // Terminal input arrives from a dedicated OS thread.
    let (event_tx, mut event_rx) = mpsc::channel::<Event>(256);
    std::thread::spawn(move || loop {
        match event::poll(Duration::from_millis(200)) {
            Ok(true) => match event::read() {
                Ok(ev) => {
                    if event_tx.blocking_send(ev).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            },
            Ok(false) => {}
            Err(_) => break,
        }
    });

    let mut spinner_tick = tokio::time::interval(Duration::from_millis(120));

    loop {
        terminal.draw(|f| draw(f, &app))?;
        if app.should_quit {
            break;
        }

        tokio::select! {
            maybe_ev = event_rx.recv() => {
                match maybe_ev {
                    Some(Event::Key(key)) => handle_key(&mut app, key, &prompt_tx),
                    Some(_) => {}
                    None => { app.should_quit = true; }
                }
            }
            Some(agent_ev) = evt_rx.recv() => {
                handle_agent_event(&mut app, agent_ev);
            }
            _ = spinner_tick.tick() => {
                if app.busy {
                    app.spinner = (app.spinner + 1) % SPINNER.len();
                }
            }
        }
    }

    Ok("已退出 OmniNova 终端。".to_string())
}

fn handle_agent_event(app: &mut App, ev: AgentEvent) {
    app.scroll_back = 0;
    match ev {
        AgentEvent::Token(t) => {
            let idx = match app.streaming_idx {
                Some(i) => i,
                None => {
                    app.transcript.push(Msg {
                        role: Role::Assistant,
                        text: String::new(),
                    });
                    let i = app.transcript.len() - 1;
                    app.streaming_idx = Some(i);
                    i
                }
            };
            app.transcript[idx].text.push_str(&t);
        }
        AgentEvent::Step(s) => {
            // Close the current bubble so post-tool text starts a fresh one.
            app.streaming_idx = None;
            if app.show_steps {
                app.transcript.push(Msg {
                    role: Role::Tool,
                    text: s,
                });
            }
        }
        AgentEvent::Done(full) => {
            match app.streaming_idx {
                Some(i) if app.transcript[i].text.trim().is_empty() => {
                    app.transcript[i].text = full;
                }
                Some(_) => {}
                None => {
                    if !full.trim().is_empty() {
                        app.transcript.push(Msg {
                            role: Role::Assistant,
                            text: full,
                        });
                    }
                }
            }
            app.streaming_idx = None;
            app.busy = false;
        }
        AgentEvent::Error(e) => {
            app.streaming_idx = None;
            app.transcript.push(Msg {
                role: Role::Error,
                text: e,
            });
            app.busy = false;
        }
        AgentEvent::ToolExecution(evt) => {
            if app.show_steps {
                match evt {
                    crate::agent::ToolExecutionEvent::Started { tool_name, summary } => {
                        app.transcript.push(Msg {
                            role: Role::Tool,
                            text: format!("⚡ {}", summary),
                        });
                    }
                    crate::agent::ToolExecutionEvent::Completed {
                        tool_name,
                        success,
                        duration_ms,
                        result_summary,
                        diff_stats,
                    } => {
                        let icon = if success { "✅" } else { "❌" };
                        let stats = diff_stats
                            .map(|d| format!(" +{} -{}", d.additions, d.deletions))
                            .unwrap_or_default();
                        app.transcript.push(Msg {
                            role: Role::Tool,
                            text: format!(
                                "{} {} 完成 ({}ms){} — {}",
                                icon, tool_name, duration_ms, stats, result_summary
                            ),
                        });
                    }
                    crate::agent::ToolExecutionEvent::FileChanged { path, additions, deletions } => {
                        app.transcript.push(Msg {
                            role: Role::Tool,
                            text: format!("📝 {} (+{}/-{})", path, additions, deletions),
                        });
                    }
                }
            }
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent, prompt_tx: &mpsc::UnboundedSender<String>) {
    if key.kind == KeyEventKind::Release {
        return;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('c') if ctrl => app.should_quit = true,
        KeyCode::Char('d') if ctrl && app.input.is_empty() => app.should_quit = true,
        KeyCode::Char('l') if ctrl => {
            app.transcript.clear();
            app.scroll_back = 0;
        }
        KeyCode::Char('t') if ctrl => {
            app.show_steps = !app.show_steps;
        }
        KeyCode::PageUp => app.scroll_back = app.scroll_back.saturating_add(5),
        KeyCode::PageDown => app.scroll_back = app.scroll_back.saturating_sub(5),
        KeyCode::Enter => {
            let text = app.input_string().trim().to_string();
            if text.is_empty() || app.busy {
                return;
            }
            app.input.clear();
            app.cursor = 0;
            app.scroll_back = 0;
            app.transcript.push(Msg {
                role: Role::User,
                text: text.clone(),
            });
            app.busy = true;
            app.streaming_idx = None;
            let _ = prompt_tx.send(text);
        }
        KeyCode::Backspace => {
            if app.cursor > 0 {
                app.cursor -= 1;
                app.input.remove(app.cursor);
            }
        }
        KeyCode::Delete => {
            if app.cursor < app.input.len() {
                app.input.remove(app.cursor);
            }
        }
        KeyCode::Left => app.cursor = app.cursor.saturating_sub(1),
        KeyCode::Right => {
            if app.cursor < app.input.len() {
                app.cursor += 1;
            }
        }
        KeyCode::Home => app.cursor = 0,
        KeyCode::End => app.cursor = app.input.len(),
        KeyCode::Char(c) => {
            app.input.insert(app.cursor, c);
            app.cursor += 1;
        }
        _ => {}
    }
}

fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(f.area());

    draw_header(f, chunks[0], app);
    draw_transcript(f, chunks[1], app);
    draw_input(f, chunks[2], app);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let status = if app.busy {
        Span::styled(
            format!("  {} 思考中…", SPINNER[app.spinner]),
            Style::default().fg(Color::Yellow),
        )
    } else {
        Span::styled("  ● 就绪", Style::default().fg(Color::Green))
    };
    let line = Line::from(vec![
        Span::styled(
            " OmniNova Claw ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("  {} · {}", app.provider, app.model)),
        status,
    ]);
    f.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn draw_transcript(f: &mut Frame, area: Rect, app: &App) {
    let inner_w = area.width.saturating_sub(2) as usize;
    let inner_h = area.height.saturating_sub(2) as usize;

    let mut lines: Vec<Line> = Vec::new();
    for msg in &app.transcript {
        let (label, color) = match msg.role {
            Role::User => ("❯ 你", Color::Cyan),
            Role::Assistant => ("✦ OmniNova", Color::Green),
            Role::System => ("· 系统", Color::DarkGray),
            Role::Error => ("✗ 错误", Color::Red),
            Role::Tool => ("⚙ 步骤", Color::Yellow),
        };
        lines.push(Line::from(Span::styled(
            label.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )));
        let style = match msg.role {
            Role::System | Role::Tool => Style::default().fg(Color::DarkGray),
            Role::Error => Style::default().fg(Color::Red),
            _ => Style::default(),
        };
        for wrapped in wrap_text(&msg.text, inner_w.saturating_sub(2).max(1)) {
            lines.push(Line::from(Span::styled(format!("  {wrapped}"), style)));
        }
        lines.push(Line::from(""));
    }
    if app.busy && app.streaming_idx.is_none() {
        lines.push(Line::from(Span::styled(
            format!("  {} 正在生成…", SPINNER[app.spinner]),
            Style::default().fg(Color::Yellow),
        )));
    }

    let total = lines.len();
    let max_top = total.saturating_sub(inner_h);
    let top = max_top.saturating_sub(app.scroll_back);

    f.render_widget(
        Paragraph::new(Text::from(lines))
            .block(Block::default().borders(Borders::ALL).title(" 对话 "))
            .scroll((top as u16, 0)),
        area,
    );
}

fn draw_input(f: &mut Frame, area: Rect, app: &App) {
    let inner_w = area.width.saturating_sub(2) as usize;
    let avail = inner_w.max(1);
    let start = if app.cursor > avail.saturating_sub(1) {
        app.cursor - avail.saturating_sub(1)
    } else {
        0
    };
    let visible: String = app.input[start..].iter().take(avail).collect();
    let cursor_x = app.cursor.saturating_sub(start);

    let title = if app.busy {
        " 输入（生成中，请稍候） "
    } else {
        " 输入（Enter 发送 · Esc 退出） "
    };
    f.render_widget(
        Paragraph::new(visible).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(if app.busy {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::Cyan)
                }),
        ),
        area,
    );

    if !app.busy {
        f.set_cursor_position(Position {
            x: area.x + 1 + cursor_x as u16,
            y: area.y + 1,
        });
    }
}

/// Wrap text to `width` display columns, honoring existing newlines.
fn wrap_text(s: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for raw in s.split('\n') {
        if raw.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut cur = String::new();
        let mut count = 0usize;
        for ch in raw.chars() {
            let w = char_width(ch);
            if count + w > width && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
                count = 0;
            }
            cur.push(ch);
            count += w;
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn char_width(c: char) -> usize {
    let cp = c as u32;
    let wide = (0x1100..=0x115F).contains(&cp)
        || (0x2E80..=0xA4CF).contains(&cp)
        || (0xAC00..=0xD7A3).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFE30..=0xFE4F).contains(&cp)
        || (0xFF00..=0xFF60).contains(&cp)
        || (0xFFE0..=0xFFE6).contains(&cp);
    if wide {
        2
    } else {
        1
    }
}
