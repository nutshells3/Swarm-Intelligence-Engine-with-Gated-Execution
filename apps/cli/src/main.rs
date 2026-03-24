use clap::{Parser, Subcommand};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

mod repl;

#[derive(Parser)]
#[command(name = "swarm", about = "Development Swarm IDE CLI")]
struct Cli {
    /// API server URL
    #[arg(long, default_value = "http://127.0.0.1:8845")]
    api_url: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive REPL mode (default if no command given)
    Interactive,
    /// Check API health
    Health,
    /// Show API metadata
    Meta,
    /// Manage objectives
    Objective {
        #[command(subcommand)]
        action: ObjectiveAction,
    },
    /// Manage tasks
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },
    /// Manage loops
    Loop {
        #[command(subcommand)]
        action: LoopAction,
    },
    /// Show recent events
    Events {
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    /// Show system status (objectives, tasks, nodes summary)
    Status,
}

#[derive(Subcommand)]
enum ObjectiveAction {
    /// Create a new objective
    Create {
        #[arg(long)]
        summary: String,
    },
    /// List all objectives
    List,
    /// Get objective details
    Get { id: String },
}

#[derive(Subcommand)]
enum TaskAction {
    /// List all tasks
    List,
    /// Get task details
    Get { id: String },
}

#[derive(Subcommand)]
enum LoopAction {
    /// Create a new loop for an objective
    Create {
        #[arg(long)]
        objective_id: String,
    },
    /// List all loops
    List,
}

// ── Response types ──────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
pub(crate) struct HealthResponse {
    pub(crate) status: String,
}

#[derive(Deserialize, Debug)]
struct MetaResponse {
    service: String,
    database_backend: String,
    database_url_present: bool,
    write_path: String,
    migrations_loaded: bool,
}

#[derive(Serialize)]
pub(crate) struct CreateObjectiveRequest {
    pub(crate) summary: String,
    pub(crate) planning_status: String,
    pub(crate) plan_gate: String,
    pub(crate) idempotency_key: String,
}

#[derive(Deserialize, Debug)]
pub(crate) struct ObjectiveResponse {
    pub(crate) objective_id: String,
    pub(crate) summary: String,
    pub(crate) planning_status: String,
    pub(crate) plan_gate: String,
    pub(crate) created_at: String,
    #[allow(dead_code)]
    pub(crate) updated_at: String,
    pub(crate) duplicated: bool,
}

#[derive(Deserialize, Debug)]
pub(crate) struct TaskResponse {
    pub(crate) task_id: String,
    #[allow(dead_code)]
    pub(crate) node_id: String,
    pub(crate) worker_role: String,
    #[allow(dead_code)]
    pub(crate) skill_pack_id: String,
    pub(crate) status: String,
    pub(crate) created_at: String,
    #[allow(dead_code)]
    pub(crate) updated_at: String,
    #[allow(dead_code)]
    pub(crate) duplicated: bool,
}

#[derive(Serialize)]
struct CreateLoopRequest {
    objective_id: String,
    active_track: String,
    idempotency_key: String,
}

#[derive(Deserialize, Debug)]
struct LoopResponse {
    loop_id: String,
    objective_id: String,
    cycle_index: i32,
    active_track: String,
    created_at: String,
    #[allow(dead_code)]
    updated_at: String,
    duplicated: bool,
}

#[derive(Deserialize, Debug)]
pub(crate) struct EventResponse {
    pub(crate) event_id: String,
    pub(crate) aggregate_kind: String,
    #[allow(dead_code)]
    pub(crate) aggregate_id: String,
    pub(crate) event_kind: String,
    #[allow(dead_code)]
    pub(crate) idempotency_key: String,
    #[allow(dead_code)]
    pub(crate) payload: Value,
    pub(crate) created_at: String,
}

// ── Helpers ─────────────────────────────────────────────────────────────

pub(crate) fn status_badge(status: &str) -> String {
    match status {
        "ok" | "active" | "running" | "completed" => format!("[{}]", status.green()),
        "pending" | "waiting" | "draft" => format!("[{}]", status.yellow()),
        "failed" | "error" => format!("[{}]", status.red()),
        other => format!("[{}]", other.cyan()),
    }
}

pub(crate) fn print_header(title: &str) {
    println!("\n{}", title.bold().underline());
}

fn print_kv(key: &str, value: &str) {
    println!("  {}: {}", key.dimmed(), value);
}

pub(crate) fn truncate_id(id: &str) -> &str {
    if id.len() > 8 { &id[..8] } else { id }
}

pub(crate) fn new_idempotency_key() -> String {
    Uuid::now_v7().to_string()
}

// ── Command handlers ────────────────────────────────────────────────────

async fn cmd_health(client: &reqwest::Client, api_url: &str) -> anyhow::Result<()> {
    let resp: HealthResponse = client
        .get(format!("{api_url}/health"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    print_header("Health");
    println!("  Status: {}", status_badge(&resp.status));
    Ok(())
}

async fn cmd_meta(client: &reqwest::Client, api_url: &str) -> anyhow::Result<()> {
    let resp: MetaResponse = client
        .get(format!("{api_url}/api/meta"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    print_header("API Metadata");
    print_kv("Service", &resp.service);
    print_kv("Database", &resp.database_backend);
    print_kv(
        "DB configured",
        if resp.database_url_present {
            "yes"
        } else {
            "no"
        },
    );
    print_kv("Write path", &resp.write_path);
    print_kv(
        "Migrations",
        if resp.migrations_loaded {
            "loaded"
        } else {
            "pending"
        },
    );
    Ok(())
}

async fn cmd_objective_create(
    client: &reqwest::Client,
    api_url: &str,
    summary: &str,
) -> anyhow::Result<()> {
    let body = CreateObjectiveRequest {
        summary: summary.to_owned(),
        planning_status: "draft".to_owned(),
        plan_gate: "needs_plan".to_owned(),
        idempotency_key: new_idempotency_key(),
    };

    let resp: ObjectiveResponse = client
        .post(format!("{api_url}/api/objectives"))
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    print_header("Objective Created");
    print_kv("ID", &resp.objective_id);
    print_kv("Summary", &resp.summary);
    print_kv("Status", &format!("{}", status_badge(&resp.planning_status)));
    print_kv("Gate", &resp.plan_gate);
    if resp.duplicated {
        println!("  {}", "(duplicate -- idempotent)".yellow());
    }
    Ok(())
}

async fn cmd_objective_list(client: &reqwest::Client, api_url: &str) -> anyhow::Result<()> {
    let resp: Vec<ObjectiveResponse> = client
        .get(format!("{api_url}/api/objectives"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    print_header(&format!("Objectives ({})", resp.len()));
    if resp.is_empty() {
        println!("  {}", "(none)".dimmed());
        return Ok(());
    }
    for obj in &resp {
        println!(
            "  {} {} {}  {}",
            truncate_id(&obj.objective_id).cyan(),
            status_badge(&obj.planning_status),
            obj.summary,
            obj.created_at.dimmed(),
        );
    }
    Ok(())
}

async fn cmd_objective_get(
    client: &reqwest::Client,
    api_url: &str,
    id: &str,
) -> anyhow::Result<()> {
    let resp: ObjectiveResponse = client
        .get(format!("{api_url}/api/objectives/{id}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    print_header("Objective");
    print_kv("ID", &resp.objective_id);
    print_kv("Summary", &resp.summary);
    print_kv("Planning status", &format!("{}", status_badge(&resp.planning_status)));
    print_kv("Plan gate", &resp.plan_gate);
    print_kv("Created", &resp.created_at);
    Ok(())
}

async fn cmd_task_list(client: &reqwest::Client, api_url: &str) -> anyhow::Result<()> {
    let resp: Vec<TaskResponse> = client
        .get(format!("{api_url}/api/tasks"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    print_header(&format!("Tasks ({})", resp.len()));
    if resp.is_empty() {
        println!("  {}", "(none)".dimmed());
        return Ok(());
    }
    for task in &resp {
        println!(
            "  {} {} {}  {}",
            truncate_id(&task.task_id).cyan(),
            status_badge(&task.status),
            task.worker_role,
            task.created_at.dimmed(),
        );
    }
    Ok(())
}

async fn cmd_task_get(
    client: &reqwest::Client,
    api_url: &str,
    id: &str,
) -> anyhow::Result<()> {
    let resp: TaskResponse = client
        .get(format!("{api_url}/api/tasks/{id}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    print_header("Task");
    print_kv("ID", &resp.task_id);
    print_kv("Node ID", &resp.node_id);
    print_kv("Worker role", &resp.worker_role);
    print_kv("Skill pack", &resp.skill_pack_id);
    print_kv("Status", &format!("{}", status_badge(&resp.status)));
    print_kv("Created", &resp.created_at);
    Ok(())
}

async fn cmd_loop_create(
    client: &reqwest::Client,
    api_url: &str,
    objective_id: &str,
) -> anyhow::Result<()> {
    let body = CreateLoopRequest {
        objective_id: objective_id.to_owned(),
        active_track: "plan".to_owned(),
        idempotency_key: new_idempotency_key(),
    };

    let resp: LoopResponse = client
        .post(format!("{api_url}/api/loops"))
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    print_header("Loop Created");
    print_kv("Loop ID", &resp.loop_id);
    print_kv("Objective ID", &resp.objective_id);
    print_kv("Cycle index", &resp.cycle_index.to_string());
    print_kv("Active track", &resp.active_track);
    if resp.duplicated {
        println!("  {}", "(duplicate -- idempotent)".yellow());
    }
    Ok(())
}

async fn cmd_loop_list(client: &reqwest::Client, api_url: &str) -> anyhow::Result<()> {
    let resp: Vec<LoopResponse> = client
        .get(format!("{api_url}/api/loops"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    print_header(&format!("Loops ({})", resp.len()));
    if resp.is_empty() {
        println!("  {}", "(none)".dimmed());
        return Ok(());
    }
    for lp in &resp {
        println!(
            "  {} obj={} cycle={} track={}  {}",
            truncate_id(&lp.loop_id).cyan(),
            truncate_id(&lp.objective_id),
            lp.cycle_index,
            lp.active_track,
            lp.created_at.dimmed(),
        );
    }
    Ok(())
}

async fn cmd_events(
    client: &reqwest::Client,
    api_url: &str,
    limit: u32,
) -> anyhow::Result<()> {
    let resp: Vec<EventResponse> = client
        .get(format!("{api_url}/api/events"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let display: Vec<&EventResponse> = resp.iter().take(limit as usize).collect();

    print_header(&format!("Events (showing {}/{})", display.len(), resp.len()));
    if display.is_empty() {
        println!("  {}", "(none)".dimmed());
        return Ok(());
    }
    for ev in &display {
        println!(
            "  {} {} {}  {}",
            truncate_id(&ev.event_id).dimmed(),
            ev.aggregate_kind.cyan(),
            ev.event_kind.bold(),
            ev.created_at.dimmed(),
        );
    }
    Ok(())
}

async fn cmd_status(client: &reqwest::Client, api_url: &str) -> anyhow::Result<()> {
    // Fetch health first to confirm connectivity
    let health: HealthResponse = client
        .get(format!("{api_url}/health"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    print_header("System Status");
    println!("  API: {}", status_badge(&health.status));

    // Objectives count
    let objectives: Vec<Value> = client
        .get(format!("{api_url}/api/objectives"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    println!("  Objectives: {}", objectives.len().to_string().bold());

    // Tasks count
    let tasks: Vec<Value> = client
        .get(format!("{api_url}/api/tasks"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    println!("  Tasks: {}", tasks.len().to_string().bold());

    // Loops count
    let loops: Vec<Value> = client
        .get(format!("{api_url}/api/loops"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    println!("  Loops: {}", loops.len().to_string().bold());

    // Nodes count
    let nodes: Vec<Value> = client
        .get(format!("{api_url}/api/nodes"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    println!("  Nodes: {}", nodes.len().to_string().bold());

    // Recent events
    let events: Vec<Value> = client
        .get(format!("{api_url}/api/events"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    println!("  Events: {}", events.len().to_string().bold());

    Ok(())
}

// ── Main ────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let client = reqwest::Client::new();
    let api_url = &cli.api_url;

    let result = match cli.command {
        None | Some(Commands::Interactive) => repl::run(api_url.to_string()).await,
        Some(Commands::Health) => cmd_health(&client, api_url).await,
        Some(Commands::Meta) => cmd_meta(&client, api_url).await,
        Some(Commands::Objective { action }) => match action {
            ObjectiveAction::Create { summary } => {
                cmd_objective_create(&client, api_url, &summary).await
            }
            ObjectiveAction::List => cmd_objective_list(&client, api_url).await,
            ObjectiveAction::Get { id } => cmd_objective_get(&client, api_url, &id).await,
        },
        Some(Commands::Task { action }) => match action {
            TaskAction::List => cmd_task_list(&client, api_url).await,
            TaskAction::Get { id } => cmd_task_get(&client, api_url, &id).await,
        },
        Some(Commands::Loop { action }) => match action {
            LoopAction::Create { objective_id } => {
                cmd_loop_create(&client, api_url, &objective_id).await
            }
            LoopAction::List => cmd_loop_list(&client, api_url).await,
        },
        Some(Commands::Events { limit }) => cmd_events(&client, api_url, limit).await,
        Some(Commands::Status) => cmd_status(&client, api_url).await,
    };

    if let Err(e) = result {
        eprintln!("{} {e}", "error:".red().bold());
        std::process::exit(1);
    }
}

pub(crate) mod anyhow {
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
}
