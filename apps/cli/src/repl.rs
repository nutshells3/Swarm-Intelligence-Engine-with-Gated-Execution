use colored::Colorize;
use futures_util::StreamExt;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use serde::Deserialize;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::anyhow;
use crate::{
    status_badge, truncate_id, CreateObjectiveRequest, EventResponse, HealthResponse,
    ObjectiveResponse, TaskResponse, new_idempotency_key,
};

// ── Session state ──────────────────────────────────────────────────────

struct Session {
    api_url: String,
    client: reqwest::Client,
    active_objective_id: Option<String>,
    active_objective_phase: Option<String>,
    tail_enabled: Arc<AtomicBool>,
}

impl Session {
    fn new(api_url: String) -> Self {
        Self {
            api_url,
            client: reqwest::Client::new(),
            active_objective_id: None,
            active_objective_phase: None,
            tail_enabled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn prompt(&self) -> String {
        match (&self.active_objective_id, &self.active_objective_phase) {
            (Some(id), Some(phase)) => {
                format!(
                    "{}{}{}{}{}> ",
                    "swarm".bold(),
                    "[".dimmed(),
                    truncate_id(id).cyan(),
                    format!("|{phase}").dimmed(),
                    "]".dimmed(),
                )
            }
            (Some(id), None) => {
                format!(
                    "{}{}{}{}> ",
                    "swarm".bold(),
                    "[".dimmed(),
                    truncate_id(id).cyan(),
                    "]".dimmed(),
                )
            }
            _ => format!("{}> ", "swarm".bold()),
        }
    }
}

// ── Status summary (used for banner) ───────────────────────────────────

#[derive(Default)]
struct StatusSummary {
    healthy: bool,
    objectives: usize,
    tasks: usize,
    queue: usize,
}

async fn fetch_status_summary(client: &reqwest::Client, api_url: &str) -> StatusSummary {
    let mut summary = StatusSummary::default();

    // Check health
    if let Ok(resp) = client.get(format!("{api_url}/health")).send().await {
        if let Ok(h) = resp.json::<HealthResponse>().await {
            summary.healthy = h.status == "ok";
        }
    }

    // Count objectives
    if let Ok(resp) = client
        .get(format!("{api_url}/api/objectives"))
        .send()
        .await
    {
        if let Ok(objs) = resp.json::<Vec<Value>>().await {
            summary.objectives = objs.len();
        }
    }

    // Count tasks
    if let Ok(resp) = client.get(format!("{api_url}/api/tasks")).send().await {
        if let Ok(tasks) = resp.json::<Vec<Value>>().await {
            summary.tasks = tasks.len();
            // queue = tasks with status "pending" or "waiting"
            summary.queue = tasks
                .iter()
                .filter(|t| {
                    t.get("status")
                        .and_then(|s| s.as_str())
                        .map(|s| s == "pending" || s == "waiting")
                        .unwrap_or(false)
                })
                .count();
        }
    }

    summary
}

// ── Startup banner ─────────────────────────────────────────────────────

fn print_banner(api_url: &str, summary: &StatusSummary) {
    let health_indicator = if summary.healthy {
        "ok".green().to_string()
    } else {
        "unreachable".red().to_string()
    };

    println!();
    println!(
        "{}",
        "+-------------------------------------------------+".dimmed()
    );
    println!(
        "{}  {}{}",
        "|".dimmed(),
        "Development Swarm IDE".bold(),
        "                          |".dimmed(),
    );
    println!(
        "{}  API: {} {}{}",
        "|".dimmed(),
        api_url,
        health_indicator,
        "  |".dimmed(),
    );
    println!(
        "{}  Objectives: {}  Tasks: {}  Queue: {}{}",
        "|".dimmed(),
        summary.objectives.to_string().bold(),
        summary.tasks.to_string().bold(),
        summary.queue.to_string().bold(),
        "         |".dimmed(),
    );
    println!(
        "{}  Type {} for commands or just start typing {}",
        "|".dimmed(),
        "/help".cyan(),
        "  |".dimmed(),
    );
    println!(
        "{}",
        "+-------------------------------------------------+".dimmed()
    );
    println!();
}

// ── SSE event streaming ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SseEvent {
    #[serde(default)]
    event_kind: String,
    #[serde(default)]
    aggregate_id: String,
    #[serde(default)]
    aggregate_kind: String,
    #[serde(default)]
    payload: Value,
}

fn format_sse_event(event_kind: &str, aggregate_id: &str, payload: &Value) -> String {
    let id_short = truncate_id(aggregate_id);
    let detail = payload
        .get("summary")
        .or_else(|| payload.get("status"))
        .or_else(|| payload.get("worker_role"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let detail_part = if detail.is_empty() {
        String::new()
    } else {
        format!(" ({})", detail)
    };

    format!(
        "  {} {}: {}{}",
        "[event]".dimmed(),
        event_kind.bold(),
        id_short.cyan(),
        detail_part,
    )
}

async fn run_sse_listener(
    api_url: String,
    tail_enabled: Arc<AtomicBool>,
    tx: mpsc::UnboundedSender<String>,
) {
    let client = reqwest::Client::new();
    loop {
        // Only attempt connection when tail is enabled
        if !tail_enabled.load(Ordering::Relaxed) {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            continue;
        }

        let resp = client
            .get(format!("{api_url}/api/events/stream"))
            .header("Accept", "text/event-stream")
            .send()
            .await;

        match resp {
            Ok(response) => {
                let mut stream = response.bytes_stream();
                let mut buffer = String::new();

                while let Some(chunk) = stream.next().await {
                    if !tail_enabled.load(Ordering::Relaxed) {
                        break;
                    }

                    match chunk {
                        Ok(bytes) => {
                            buffer.push_str(&String::from_utf8_lossy(&bytes));

                            // Parse SSE lines: look for "data: {...}" lines
                            loop {
                                let Some(data_start) = buffer.find("data: ") else {
                                    break;
                                };
                                let rest_start = data_start + 6;
                                let Some(newline_pos) =
                                    buffer[rest_start..].find('\n')
                                else {
                                    // Incomplete line, wait for more data
                                    break;
                                };
                                let line_end = rest_start + newline_pos;
                                let json_owned =
                                    buffer[rest_start..line_end].trim().to_owned();
                                buffer = buffer[line_end + 1..].to_string();

                                if let Ok(event) =
                                    serde_json::from_str::<SseEvent>(&json_owned)
                                {
                                    let msg = format_sse_event(
                                        &event.event_kind,
                                        &event.aggregate_id,
                                        &event.payload,
                                    );
                                    let _ = tx.send(msg);
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
            Err(_) => {
                // Connection failed, wait before retry
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }

        // Brief pause before reconnecting
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

// ── Slash-command handlers ─────────────────────────────────────────────

async fn cmd_help() {
    println!();
    println!("{}", "Available commands:".bold().underline());
    println!(
        "  {}          -- show this help message",
        "/help".cyan()
    );
    println!(
        "  {}        -- show system status (objectives, tasks, queue, agents)",
        "/status".cyan()
    );
    println!(
        "  {}    -- list objectives",
        "/objectives".cyan()
    );
    println!(
        "  {}         -- list tasks with colored status",
        "/tasks".cyan()
    );
    println!(
        "  {}    -- show recent events (default 20)",
        "/events [n]".cyan()
    );
    println!(
        "  {}         -- list nodes with lane/lifecycle",
        "/nodes".cyan()
    );
    println!(
        "  {}        -- show current execution policy",
        "/policy".cyan()
    );
    println!(
        "  {}  -- show plan gate status for an objective",
        "/gate [obj_id]".cyan()
    );
    println!(
        "  {}          -- toggle real-time event tail on/off",
        "/tail".cyan()
    );
    println!(
        "  {}         -- clear screen",
        "/clear".cyan()
    );
    println!(
        "  {}          -- exit",
        "/quit".cyan()
    );
    println!();
    println!(
        "{}",
        "Or just type plain text to create/update an objective.".dimmed()
    );
    println!();
}

async fn cmd_status_repl(session: &Session) -> anyhow::Result<()> {
    let summary = fetch_status_summary(&session.client, &session.api_url).await;

    println!();
    crate::print_header("System Status");
    let health_str = if summary.healthy { "ok" } else { "down" };
    println!("  API: {} {}", &session.api_url, status_badge(health_str));
    println!("  Objectives: {}", summary.objectives.to_string().bold());
    println!("  Tasks: {}", summary.tasks.to_string().bold());
    println!("  Queue: {}", summary.queue.to_string().bold());

    // Also show active objective if any
    if let Some(ref id) = session.active_objective_id {
        println!("  Active objective: {}", truncate_id(id).cyan());
    }
    println!();
    Ok(())
}

async fn cmd_objectives_repl(session: &Session) -> anyhow::Result<()> {
    let resp: Vec<ObjectiveResponse> = session
        .client
        .get(format!("{}/api/objectives", session.api_url))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    println!();
    crate::print_header(&format!("Objectives ({})", resp.len()));
    if resp.is_empty() {
        println!("  {}", "(none)".dimmed());
    } else {
        for obj in &resp {
            let marker = if session
                .active_objective_id
                .as_deref()
                .map(|a| a == obj.objective_id)
                .unwrap_or(false)
            {
                ">> ".green().to_string()
            } else {
                "   ".to_string()
            };
            println!(
                "{}{} {} {}  {}",
                marker,
                truncate_id(&obj.objective_id).cyan(),
                status_badge(&obj.planning_status),
                obj.summary,
                obj.created_at.dimmed(),
            );
        }
    }
    println!();
    Ok(())
}

async fn cmd_tasks_repl(session: &Session) -> anyhow::Result<()> {
    let resp: Vec<TaskResponse> = session
        .client
        .get(format!("{}/api/tasks", session.api_url))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    println!();
    crate::print_header(&format!("Tasks ({})", resp.len()));
    if resp.is_empty() {
        println!("  {}", "(none)".dimmed());
    } else {
        for task in &resp {
            println!(
                "  {} {} {}  {}",
                truncate_id(&task.task_id).cyan(),
                status_badge(&task.status),
                task.worker_role,
                task.created_at.dimmed(),
            );
        }
    }
    println!();
    Ok(())
}

async fn cmd_events_repl(session: &Session, limit: usize) -> anyhow::Result<()> {
    let resp: Vec<EventResponse> = session
        .client
        .get(format!("{}/api/events", session.api_url))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let display: Vec<&EventResponse> = resp.iter().take(limit).collect();

    println!();
    crate::print_header(&format!(
        "Events (showing {}/{})",
        display.len(),
        resp.len()
    ));
    if display.is_empty() {
        println!("  {}", "(none)".dimmed());
    } else {
        for ev in &display {
            println!(
                "  {} {} {}  {}",
                truncate_id(&ev.event_id).dimmed(),
                ev.aggregate_kind.cyan(),
                ev.event_kind.bold(),
                ev.created_at.dimmed(),
            );
        }
    }
    println!();
    Ok(())
}

#[derive(Deserialize, Debug)]
struct NodeResponse {
    node_id: String,
    #[allow(dead_code)]
    objective_id: String,
    lane: String,
    lifecycle: String,
    #[allow(dead_code)]
    created_at: String,
}

async fn cmd_nodes_repl(session: &Session) -> anyhow::Result<()> {
    let resp: Vec<NodeResponse> = session
        .client
        .get(format!("{}/api/nodes", session.api_url))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    println!();
    crate::print_header(&format!("Nodes ({})", resp.len()));
    if resp.is_empty() {
        println!("  {}", "(none)".dimmed());
    } else {
        for node in &resp {
            println!(
                "  {} lane={} lifecycle={}",
                truncate_id(&node.node_id).cyan(),
                node.lane.bold(),
                status_badge(&node.lifecycle),
            );
        }
    }
    println!();
    Ok(())
}

async fn cmd_policy_repl(session: &Session) -> anyhow::Result<()> {
    // Try to fetch policy; this endpoint may or may not exist
    let result = session
        .client
        .get(format!("{}/api/policy", session.api_url))
        .send()
        .await;

    println!();
    crate::print_header("Execution Policy");
    match result {
        Ok(resp) => {
            if resp.status().is_success() {
                let body: Value = resp.json().await?;
                println!(
                    "  {}",
                    serde_json::to_string_pretty(&body).unwrap_or_else(|_| "(unknown)".into())
                );
            } else {
                println!(
                    "  {}",
                    "Policy endpoint not available (HTTP status error)."
                        .yellow()
                );
            }
        }
        Err(_) => {
            println!("  {}", "Policy endpoint not reachable.".yellow());
        }
    }
    println!();
    Ok(())
}

async fn cmd_gate_repl(session: &Session, obj_id: Option<&str>) -> anyhow::Result<()> {
    let id = match obj_id.or(session.active_objective_id.as_deref()) {
        Some(id) => id.to_string(),
        None => {
            println!(
                "  {}",
                "No objective specified and no active objective. Usage: /gate <obj_id>".yellow()
            );
            return Ok(());
        }
    };

    let resp: ObjectiveResponse = session
        .client
        .get(format!("{}/api/objectives/{}", session.api_url, id))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    println!();
    crate::print_header(&format!("Plan Gate: {}", truncate_id(&resp.objective_id)));
    println!("  Summary: {}", resp.summary);
    println!("  Gate: {}", resp.plan_gate.bold());
    println!("  Planning status: {}", status_badge(&resp.planning_status));

    // Simulated gate checklist based on plan_gate value
    let gate_open = resp.plan_gate != "needs_plan";
    let is_drafted = resp.planning_status != "draft";
    let summarized = !resp.summary.is_empty();

    println!();
    println!("  {} Plan gate: {}", if gate_open { "OPEN".green() } else { "BLOCKED".red() }, "");
    print_gate_item("objective_summarized", summarized);
    print_gate_item("architecture_drafted", is_drafted);
    print_gate_item("milestones_created", gate_open);
    print_gate_item("acceptance_criteria_defined", gate_open);
    println!();
    Ok(())
}

fn print_gate_item(label: &str, satisfied: bool) {
    if satisfied {
        println!("   {} {}", "ok".green(), label);
    } else {
        println!("   {} {}", "--".red(), label);
    }
}

fn cmd_tail_toggle(session: &Session) {
    let was = session.tail_enabled.load(Ordering::Relaxed);
    session.tail_enabled.store(!was, Ordering::Relaxed);
    if !was {
        println!(
            "  {} Real-time event tail {}",
            ">>".green(),
            "ON".green().bold()
        );
    } else {
        println!(
            "  {} Real-time event tail {}",
            "--".dimmed(),
            "OFF".yellow().bold()
        );
    }
}

// ── Natural language input handling ────────────────────────────────────

/// Heuristic extraction from user input text.
/// Returns (constraints, decisions, open_questions).
fn extract_from_input(text: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut constraints = Vec::new();
    let mut decisions = Vec::new();
    let mut open_questions = Vec::new();

    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();

    // Look for technology/tool mentions as constraints
    let tech_keywords = [
        "jwt", "bcrypt", "oauth", "redis", "postgres", "mysql", "sqlite", "docker",
        "kubernetes", "k8s", "graphql", "rest", "grpc", "websocket", "https", "tls",
        "ssl", "sha256", "aes", "rsa", "hmac", "argon2",
    ];
    for kw in &tech_keywords {
        if lower.contains(kw) {
            constraints.push(format!("{}-based", kw));
        }
    }

    // Look for action verbs that imply decisions
    let decision_patterns = [
        ("use ", "decision to use"),
        ("implement ", "implement"),
        ("build ", "build"),
        ("create ", "create"),
        ("add ", "add"),
        ("setup ", "setup"),
        ("configure ", "configure"),
        ("deploy ", "deploy"),
        ("integrate ", "integrate"),
    ];
    for (pat, label) in &decision_patterns {
        if lower.contains(pat) {
            // Extract a brief context after the pattern
            if let Some(idx) = lower.find(pat) {
                let rest = &text[idx + pat.len()..];
                let snippet: String = rest.split(',').next().unwrap_or(rest).trim().to_string();
                if !snippet.is_empty() && snippet.len() < 60 {
                    decisions.push(format!("{}: {}", label, snippet));
                }
            }
        }
    }

    // Look for question marks or uncertain language
    if text.contains('?') {
        for sentence in text.split('?') {
            let trimmed = sentence.trim();
            if !trimmed.is_empty() && trimmed.len() < 80 {
                open_questions.push(format!("{}?", trimmed));
            }
        }
    }

    // If we found nothing, add a generic decision from the full text
    if constraints.is_empty() && decisions.is_empty() {
        // Extract nouns/phrases as potential architectural subjects
        let key_nouns = [
            "auth", "authentication", "authorization", "user", "api", "database",
            "cache", "queue", "worker", "service", "model", "schema", "migration",
            "endpoint", "route", "middleware", "controller", "handler", "storage",
            "token", "session", "password", "permission", "role",
        ];
        for noun in &key_nouns {
            if words.contains(noun) {
                decisions.push(format!("{} system architecture", noun));
            }
        }
    }

    // Ensure we have at least one item to display
    if constraints.is_empty() && decisions.is_empty() && open_questions.is_empty() {
        decisions.push("system design from input".to_string());
    }

    // De-duplicate
    constraints.dedup();
    decisions.dedup();
    open_questions.dedup();

    (constraints, decisions, open_questions)
}

fn print_extraction(constraints: &[String], decisions: &[String], open_questions: &[String]) {
    let total = constraints.len() + decisions.len() + open_questions.len();
    let mut idx = 0;

    for c in constraints {
        idx += 1;
        let prefix = if idx == total {
            "    \u{2514}\u{2500}\u{2500}"
        } else {
            "    \u{251c}\u{2500}\u{2500}"
        };
        println!(
            "{} {} constraint: {}",
            prefix,
            ">>".yellow(),
            c
        );
    }

    for d in decisions {
        idx += 1;
        let prefix = if idx == total {
            "    \u{2514}\u{2500}\u{2500}"
        } else {
            "    \u{251c}\u{2500}\u{2500}"
        };
        println!(
            "{} {} decision: {}",
            prefix,
            ">>".cyan(),
            d
        );
    }

    for q in open_questions {
        idx += 1;
        let prefix = if idx == total {
            "    \u{2514}\u{2500}\u{2500}"
        } else {
            "    \u{251c}\u{2500}\u{2500}"
        };
        println!(
            "{} {} open question: {}",
            prefix,
            "??".yellow(),
            q
        );
    }
}

async fn handle_natural_language(session: &mut Session, input: &str) -> anyhow::Result<()> {
    let (constraints, decisions, open_questions) = extract_from_input(input);

    if session.active_objective_id.is_some() {
        // Absorbed into existing objective
        let obj_id = session.active_objective_id.as_ref().unwrap().clone();
        println!();
        println!(
            "  {} Absorbed into objective {}",
            "<<".green(),
            truncate_id(&obj_id).cyan(),
        );
        print_extraction(&constraints, &decisions, &open_questions);
        println!();
    } else {
        // Create a new objective
        let body = CreateObjectiveRequest {
            summary: input.to_owned(),
            planning_status: "draft".to_owned(),
            plan_gate: "needs_plan".to_owned(),
            idempotency_key: new_idempotency_key(),
        };

        let result = session
            .client
            .post(format!("{}/api/objectives", session.api_url))
            .json(&body)
            .send()
            .await;

        match result {
            Ok(resp) => {
                if resp.status().is_success() {
                    let obj: ObjectiveResponse = resp.json().await?;
                    session.active_objective_id = Some(obj.objective_id.clone());
                    session.active_objective_phase = Some("planning".to_string());

                    println!();
                    println!(
                        "  {} Objective created: {}",
                        "<<".green(),
                        truncate_id(&obj.objective_id).cyan(),
                    );
                    println!("     Summary: {}", obj.summary);
                    println!();
                    println!(
                        "  {} Extracting from input...",
                        ">>".cyan(),
                    );
                    print_extraction(&constraints, &decisions, &open_questions);
                    println!();

                    // Gate status display
                    let summarized = !obj.summary.is_empty();
                    let gate_open = obj.plan_gate != "needs_plan";
                    println!(
                        "  {} Plan gate: {}",
                        ">>".cyan(),
                        if gate_open {
                            "OPEN".green().to_string()
                        } else {
                            "BLOCKED".red().to_string()
                        }
                    );
                    print_gate_item("objective_summarized", summarized);
                    print_gate_item("architecture_drafted", false);
                    print_gate_item("milestones_created", false);
                    print_gate_item("acceptance_criteria_defined", false);
                    println!();
                } else {
                    let status = resp.status();
                    let body_text = resp.text().await.unwrap_or_default();
                    println!(
                        "  {} Failed to create objective (HTTP {}): {}",
                        "!!".red(),
                        status,
                        body_text,
                    );
                }
            }
            Err(e) => {
                println!(
                    "  {} API unreachable: {}",
                    "!!".red(),
                    e,
                );
            }
        }
    }

    Ok(())
}

// ── Dispatch slash commands ────────────────────────────────────────────

async fn dispatch_command(session: &mut Session, input: &str) -> anyhow::Result<bool> {
    let parts: Vec<&str> = input.trim().splitn(2, ' ').collect();
    let cmd = parts[0];
    let arg = parts.get(1).map(|s| s.trim());

    match cmd {
        "/help" => {
            cmd_help().await;
        }
        "/status" => {
            if let Err(e) = cmd_status_repl(session).await {
                println!("  {} {}", "error:".red().bold(), e);
            }
        }
        "/objectives" => {
            if let Err(e) = cmd_objectives_repl(session).await {
                println!("  {} {}", "error:".red().bold(), e);
            }
        }
        "/tasks" => {
            if let Err(e) = cmd_tasks_repl(session).await {
                println!("  {} {}", "error:".red().bold(), e);
            }
        }
        "/events" => {
            let limit: usize = arg.and_then(|a| a.parse().ok()).unwrap_or(20);
            if let Err(e) = cmd_events_repl(session, limit).await {
                println!("  {} {}", "error:".red().bold(), e);
            }
        }
        "/nodes" => {
            if let Err(e) = cmd_nodes_repl(session).await {
                println!("  {} {}", "error:".red().bold(), e);
            }
        }
        "/policy" => {
            if let Err(e) = cmd_policy_repl(session).await {
                println!("  {} {}", "error:".red().bold(), e);
            }
        }
        "/gate" => {
            if let Err(e) = cmd_gate_repl(session, arg).await {
                println!("  {} {}", "error:".red().bold(), e);
            }
        }
        "/tail" => {
            cmd_tail_toggle(session);
        }
        "/clear" => {
            // ANSI clear screen
            print!("\x1B[2J\x1B[1;1H");
        }
        "/quit" | "/exit" | "/q" => {
            println!("  {}", "Goodbye.".dimmed());
            return Ok(true); // signal exit
        }
        other => {
            println!(
                "  {} Unknown command: {}. Type {} for help.",
                "??".yellow(),
                other.red(),
                "/help".cyan(),
            );
        }
    }

    Ok(false) // don't exit
}

// ── Main REPL entry point ──────────────────────────────────────────────

pub async fn run(api_url: String) -> anyhow::Result<()> {
    let mut session = Session::new(api_url.clone());

    // Fetch status for banner
    let summary = fetch_status_summary(&session.client, &session.api_url).await;
    print_banner(&session.api_url, &summary);

    if !summary.healthy {
        println!(
            "  {} API at {} is unreachable. Commands may fail.",
            "!!".red().bold(),
            session.api_url.yellow(),
        );
        println!(
            "  {} You can still type commands; they will retry on each invocation.",
            "..".dimmed(),
        );
        println!();
    }

    // Set up SSE event channel
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let sse_tail = session.tail_enabled.clone();
    let sse_url = session.api_url.clone();

    // Spawn SSE listener in background
    tokio::spawn(async move {
        run_sse_listener(sse_url, sse_tail, tx).await;
    });

    // Set up rustyline editor
    let mut rl = DefaultEditor::new().map_err(|e| -> Box<dyn std::error::Error> {
        format!("Failed to initialize line editor: {e}").into()
    })?;

    loop {
        // Drain any pending SSE events before showing prompt
        while let Ok(msg) = rx.try_recv() {
            println!("{msg}");
        }

        let prompt = session.prompt();

        // We need to run readline in a blocking context since it blocks the thread
        let readline = tokio::task::spawn_blocking({
            let prompt = prompt.clone();
            let mut rl_moved = std::mem::replace(
                &mut rl,
                DefaultEditor::new().map_err(|e| -> Box<dyn std::error::Error> {
                    format!("Failed to initialize line editor: {e}").into()
                })?,
            );
            move || {
                let result = rl_moved.readline(&prompt);
                (rl_moved, result)
            }
        })
        .await
        .map_err(|e| -> Box<dyn std::error::Error> {
            format!("Task join error: {e}").into()
        })?;

        let (rl_back, result) = readline;
        rl = rl_back;

        match result {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(trimmed);

                if trimmed.starts_with('/') {
                    match dispatch_command(&mut session, trimmed).await {
                        Ok(true) => break,  // /quit
                        Ok(false) => {}
                        Err(e) => {
                            println!("  {} {}", "error:".red().bold(), e);
                        }
                    }
                } else {
                    // Natural language input
                    if let Err(e) = handle_natural_language(&mut session, trimmed).await {
                        println!("  {} {}", "error:".red().bold(), e);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C: cancel current input, continue REPL
                println!("  {}", "(interrupted -- type /quit to exit)".dimmed());
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D: exit
                println!("  {}", "Goodbye.".dimmed());
                break;
            }
            Err(e) => {
                println!("  {} Input error: {}", "!!".red().bold(), e);
                break;
            }
        }
    }

    Ok(())
}
