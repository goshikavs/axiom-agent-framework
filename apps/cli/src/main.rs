//! `goshikq` CLI: inspect a running runtime and run local SDK examples.
//! Agent execution still happens in the Rust runtime via the worker protocol.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser, Debug)]
#[command(name = "goshikq", version, about = "Goshikq runtime CLI")]
struct Cli {
    /// Runtime base URL (default GOSHIKQ_RUNTIME_URL or http://127.0.0.1:7400).
    #[arg(
        long,
        env = "GOSHIKQ_RUNTIME_URL",
        default_value = "http://127.0.0.1:7400"
    )]
    endpoint: String,
    /// Bearer token when GOSHIKQ_AUTH_MODE=api_key. Never printed.
    #[arg(long, env = "GOSHIKQ_API_KEY")]
    api_key: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Check runtime, database, and local toolchain.
    Doctor,
    /// List recent runs.
    #[command(subcommand)]
    Runs(RunsCmd),
    /// Print the verification report for a run.
    Verify { run_id: String },
    /// List saved agent definitions.
    #[command(subcommand)]
    Agents(AgentsCmd),
    /// Print the OpenAPI document URL.
    Openapi,
    /// Copy a working agent template into DIR (default: .).
    Init {
        #[arg(long, default_value = "typescript")]
        template: String,
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Run `bun run dev` in the current package (TypeScript agents / Studio).
    Dev,
    /// Run an example or package (`bun run start` or `python main.py`).
    Run { path: PathBuf },
    /// Print the Studio URL; starts `bun run dev` in apps/studio when it is down.
    Studio,
    /// Validate, publish, or run saved workflow graphs.
    #[command(subcommand)]
    Workflows(WorkflowsCmd),
}

#[derive(Subcommand, Debug)]
enum RunsCmd {
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
    Inspect {
        run_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum AgentsCmd {
    List,
}

#[derive(Subcommand, Debug)]
enum WorkflowsCmd {
    /// Validate a workflow JSON file without saving it.
    Validate { file: PathBuf },
    /// Save and publish a workflow JSON file.
    Publish { file: PathBuf },
    /// Start a published workflow by id (UUID).
    Run {
        workflow_id: String,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        version: Option<i32>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let mut cli = Cli::parse();
    if std::env::var("GOSHIKQ_RUNTIME_URL").is_err() {
        if let Ok(legacy) = std::env::var("AXIOM_RUNTIME_URL") {
            if !legacy.trim().is_empty() && cli.endpoint == "http://127.0.0.1:7400" {
                cli.endpoint = legacy;
            }
        }
    }
    if cli.api_key.is_none() {
        if let Ok(legacy) = std::env::var("AXIOM_API_KEY") {
            if !legacy.trim().is_empty() {
                cli.api_key = Some(legacy);
            }
        }
    }
    let client = Client::new(&cli.endpoint, cli.api_key.as_deref());
    match cli.command {
        Commands::Doctor => doctor(&client).await,
        Commands::Runs(RunsCmd::List { status, limit }) => {
            let mut q = vec![("limit", limit.to_string())];
            if let Some(s) = status {
                q.push(("status", s));
            }
            let body = client.get_query("/api/v1/runs", &q).await?;
            print_runs(&body);
            Ok(())
        }
        Commands::Runs(RunsCmd::Inspect { run_id }) => {
            let run = client.get(&format!("/api/v1/runs/{run_id}")).await?;
            println!("{}", serde_json::to_string_pretty(&run)?);
            Ok(())
        }
        Commands::Verify { run_id } => {
            let report = client
                .get(&format!("/api/v1/runs/{run_id}/verification"))
                .await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        Commands::Agents(AgentsCmd::List) => {
            let body = client.get("/api/v1/agents").await?;
            println!("{}", serde_json::to_string_pretty(&body)?);
            Ok(())
        }
        Commands::Openapi => {
            println!("{}/api/v1/openapi.json", cli.endpoint.trim_end_matches('/'));
            Ok(())
        }
        Commands::Init { template, dir } => init_template(&template, &dir),
        Commands::Dev => run_dev(),
        Commands::Run { path } => run_example(&path),
        Commands::Studio => studio().await,
        Commands::Workflows(cmd) => match cmd {
            WorkflowsCmd::Validate { file } => workflows_validate(&client, &file).await,
            WorkflowsCmd::Publish { file } => workflows_publish(&client, &file).await,
            WorkflowsCmd::Run {
                workflow_id,
                input,
                version,
            } => workflows_run(&client, &workflow_id, input.as_deref(), version).await,
        },
    }
}

struct Client {
    http: reqwest::Client,
    base: String,
}

impl Client {
    fn new(endpoint: &str, api_key: Option<&str>) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            "goshikq-cli/0.1.0-alpha".parse().unwrap(),
        );
        if let Some(k) = api_key {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {k}").parse().unwrap(),
            );
        }
        Client {
            http: reqwest::Client::builder()
                .default_headers(headers)
                .build()
                .expect("client"),
            base: endpoint.trim_end_matches('/').to_string(),
        }
    }

    async fn get(&self, path: &str) -> Result<Value> {
        self.request("GET", path, &[]).await
    }

    async fn get_query(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        let q: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.request("GET", path, &q).await
    }

    async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        self.request_body("POST", path, body).await
    }

    #[allow(dead_code)]
    async fn put(&self, path: &str, body: &Value) -> Result<Value> {
        self.request_body("PUT", path, body).await
    }

    async fn request_body(&self, method: &str, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}{path}", self.base);
        let resp = self
            .http
            .request(method.parse()?, &url)
            .json(body)
            .send()
            .await
            .with_context(|| format!("connecting to {}", self.base))?;
        self.decode(method, path, resp).await
    }

    async fn request(&self, method: &str, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let url = format!("{}{path}", self.base);
        let mut req = self.http.request(method.parse()?, &url);
        if !query.is_empty() {
            req = req.query(query);
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("connecting to {}", self.base))?;
        self.decode(method, path, resp).await
    }

    async fn decode(&self, method: &str, path: &str, resp: reqwest::Response) -> Result<Value> {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!("{method} {path} -> {status}: {}", truncate(&text, 400));
        }
        if text.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).with_context(|| format!("decoding {path}"))
    }
}

async fn doctor(client: &Client) -> Result<()> {
    println!("Goshikq doctor");
    println!("  endpoint:     {}", client.base);
    match client.get("/health").await {
        Ok(h) => println!(
            "  runtime:      ok (version {})",
            h.get("version").and_then(|v| v.as_str()).unwrap_or("?")
        ),
        Err(e) => {
            println!("  runtime:      unreachable ({e})");
            println!("  hint:         cargo run -p goshikq-server   (DATABASE_URL=embedded)");
            std::process::exit(1);
        }
    }
    match client.get("/ready").await {
        Ok(r) => {
            println!(
                "  database:     {}",
                r.get("database").and_then(|v| v.as_str()).unwrap_or("?")
            );
            println!(
                "  auth_mode:    {}",
                r.get("auth_mode").and_then(|v| v.as_str()).unwrap_or("?")
            );
            println!(
                "  active_runs:  {}",
                r.get("active_runs").unwrap_or(&Value::Null)
            );
        }
        Err(e) => println!("  ready:        {e}"),
    }
    println!(
        "  rustc:        {}",
        command_version("rustc", &["--version"])
    );
    println!(
        "  python:       {}",
        command_version("python", &["--version"])
    );
    println!("  bun:          {}", command_version("bun", &["--version"]));
    let openai = std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|s| !s.is_empty());
    let anthropic = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|s| !s.is_empty());
    println!(
        "  OPENAI_API_KEY:     {}",
        if openai.is_some() {
            "set"
        } else {
            "absent (mock model still works)"
        }
    );
    println!(
        "  ANTHROPIC_API_KEY:  {}",
        if anthropic.is_some() {
            "set"
        } else {
            "absent (mock model still works)"
        }
    );
    Ok(())
}

fn print_runs(body: &Value) {
    let items = body
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        println!("(no runs)");
        return;
    }
    println!(
        "{:<8} {:<22} {:<18} {:<16} {:>8} {}",
        "STATUS", "ID", "AGENT", "VERIFY", "STEPS", "GOAL"
    );
    for r in items {
        let id = r.get("id").and_then(|v| v.as_str()).unwrap_or("-");
        let short = if id.len() > 22 { &id[..22] } else { id };
        println!(
            "{:<8} {:<22} {:<18} {:<16} {:>8} {}",
            r.get("status").and_then(|v| v.as_str()).unwrap_or("-"),
            short,
            truncate(
                r.get("agent_name").and_then(|v| v.as_str()).unwrap_or("-"),
                18
            ),
            r.get("verification_status")
                .and_then(|v| v.as_str())
                .unwrap_or("-"),
            r.get("step_count").and_then(|v| v.as_i64()).unwrap_or(0),
            truncate(
                r.get("goal_description")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                48
            ),
        );
    }
}

fn command_version(bin: &str, args: &[&str]) -> String {
    match std::process::Command::new(bin).args(args).output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Ok(o) => format!("error: {}", String::from_utf8_lossy(&o.stderr).trim()),
        Err(_) => "not found".into(),
    }
}

fn init_template(template: &str, dir: &Path) -> Result<()> {
    let root = find_repo_root()?;
    let (src, next) = match template {
        "typescript" => (
            root.join("templates/typescript-agent"),
            vec![
                "bun install",
                "bun run start          # requires goshikq-server on :7400",
            ],
        ),
        "python" => (
            root.join("templates/python-agent"),
            vec![
                "pip install -e ../../sdk/python",
                "python main.py          # requires goshikq-server on :7400",
            ],
        ),
        other => bail!("unsupported template `{other}` (python|typescript)"),
    };
    if !src.is_dir() {
        bail!("template missing at {}", src.display());
    }
    std::fs::create_dir_all(dir)?;
    copy_dir(&src, dir)?;
    println!("created {template} agent template in {}", dir.display());
    println!("next:");
    for line in next {
        println!("  {line}");
    }
    Ok(())
}

async fn workflows_validate(client: &Client, file: &Path) -> Result<()> {
    let def = read_json_file(file)?;
    let body = client
        .post(
            "/api/v1/workflows/validate",
            &serde_json::json!({"definition": def, "draft": true}),
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&body)?);
    if body.get("valid").and_then(|v| v.as_bool()) != Some(true) {
        std::process::exit(2);
    }
    Ok(())
}

async fn workflows_publish(client: &Client, file: &Path) -> Result<()> {
    let def = read_json_file(file)?;
    let created = match client
        .post(
            "/api/v1/workflows",
            &serde_json::json!({"definition": def.clone(), "draft": true}),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            let name = def.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let listed = client.get("/api/v1/workflows").await?;
            let id = listed
                .get("items")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .find(|w| w.get("name").and_then(|n| n.as_str()) == Some(name))
                .and_then(|w| w.get("id"))
                .and_then(|v| v.as_str())
                .with_context(|| {
                    format!("create failed and no existing workflow named `{name}`: {e}")
                })?;
            client
                .put(
                    &format!("/api/v1/workflows/{id}"),
                    &serde_json::json!({"definition": def, "draft": true}),
                )
                .await?;
            serde_json::json!({"id": id})
        }
    };
    let id = created
        .get("id")
        .and_then(|v| v.as_str())
        .context("create workflow missing id")?;
    let published = client
        .post(
            &format!("/api/v1/workflows/{id}/publish"),
            &Value::Object(Default::default()),
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&published)?);
    Ok(())
}

async fn workflows_run(
    client: &Client,
    workflow_id: &str,
    input: Option<&Path>,
    version: Option<i32>,
) -> Result<()> {
    let payload = match input {
        Some(p) => read_json_file(p)?,
        None => json!({}),
    };
    let mut body = serde_json::json!({"input": payload});
    if let Some(v) = version {
        body["version"] = json!(v);
    }
    let resp = client
        .post(&format!("/api/v1/workflows/{workflow_id}/run"), &body)
        .await?;
    println!("{}", serde_json::to_string_pretty(&resp)?);
    Ok(())
}

fn read_json_file(path: &Path) -> Result<Value> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn run_dev() -> Result<()> {
    if !Path::new("package.json").exists() {
        bail!(
            "no package.json in {}; run this inside a TypeScript agent or apps/studio",
            std::env::current_dir()?.display()
        );
    }
    let status = Command::new("bun")
        .args(["run", "dev"])
        .status()
        .context("bun not found")?;
    if !status.success() {
        bail!("bun run dev exited {status}");
    }
    Ok(())
}

fn run_example(path: &Path) -> Result<()> {
    let target = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    };
    if target.join("package.json").exists() {
        let status = Command::new("bun")
            .args(["run", "start"])
            .current_dir(&target)
            .status()
            .context("bun not found")?;
        if !status.success() {
            bail!("bun run start exited {status}");
        }
        return Ok(());
    }
    if target.join("main.py").exists() {
        let status = Command::new("python")
            .arg("main.py")
            .current_dir(&target)
            .status()
            .context("python not found")?;
        if !status.success() {
            bail!("python main.py exited {status}");
        }
        return Ok(());
    }
    bail!("no package.json or main.py under {}", path.display());
}

async fn studio() -> Result<()> {
    let url =
        std::env::var("GOSHIKQ_STUDIO_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".into());
    if http_ok(&url).await {
        println!("Goshikq Studio is up at {url}");
        return Ok(());
    }
    let root = find_repo_root()?;
    let studio_dir = root.join("apps/studio");
    if !studio_dir.join("package.json").exists() {
        bail!("Studio sources not found at {}", studio_dir.display());
    }
    println!("Studio not reachable at {url}; starting `bun run dev` in apps/studio");
    println!("open {url} after Next.js prints Ready");
    let status = Command::new("bun")
        .args(["run", "dev"])
        .current_dir(&studio_dir)
        .status()
        .context("bun not found")?;
    if !status.success() {
        bail!("bun run dev (studio) exited {status}");
    }
    Ok(())
}

async fn http_ok(url: &str) -> bool {
    reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map(|r| r.status().is_success() || r.status().is_redirection())
        .unwrap_or(false)
}

fn find_repo_root() -> Result<PathBuf> {
    let mut cur = std::env::current_dir()?;
    for _ in 0..12 {
        if cur.join("Cargo.toml").exists() && cur.join("sdk/typescript").exists() {
            return Ok(cur);
        }
        if !cur.pop() {
            break;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut cur = exe;
        for _ in 0..12 {
            cur.pop();
            if cur.join("Cargo.toml").exists() && cur.join("sdk/typescript").exists() {
                return Ok(cur);
            }
        }
    }
    bail!("could not find the Goshikq repository root (looked for Cargo.toml + sdk/typescript)");
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            if entry.file_name() == "node_modules" {
                continue;
            }
            std::fs::create_dir_all(&to)?;
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{t}…")
    }
}
