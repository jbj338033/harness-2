// IMPLEMENTS: D-202, D-207, D-208
mod auth_login;
mod config_explain;
mod daemon_rpc;
mod progress;
mod style;
mod table;
mod ws_client;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use dialoguer::{Confirm, Input, Password, Select};
use harness_auth::pairing::{self, DeviceRecord};
use harness_auth::{PrivateKey, PublicKey, generate_keypair};
use harness_lifecycle::{DataDir, ModelRegistry, data_dir};
use harness_storage::{Database, Writer, WriterHandle};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(
    name = "harness",
    bin_name = "harness",
    version,
    about = "persistent, multi-agent AI coding daemon",
    long_about = "harness is a persistent, multi-agent AI coding daemon.\n\
                  Run `harness` with no command to attach to a running daemon (tui).\n\
                  Use subcommands to manage credentials, devices, and configuration.",
    override_usage = "harness [options]\n       harness [options] <command>",
    after_help = "Examples:\n  \
                  harness                attach to running daemon (tui)\n  \
                  harness init           first-run setup\n  \
                  harness auth login     sign in a provider\n  \
                  harness pair           issue a pairing code",
    arg_required_else_help = false,
    args_conflicts_with_subcommands = true,
    styles = style::clap_styles(),
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub session: SessionFlags,

    /// disable colors and unicode glyphs (for ci, logs, screen readers)
    #[arg(long, global = true)]
    pub plain: bool,
}

#[derive(clap::Args, Debug, Default)]
pub struct SessionFlags {
    /// resume cwd's latest session
    #[arg(short = 'c', long = "continue")]
    pub continue_session: bool,

    /// resume session by id; omit to pick interactively
    #[arg(short = 'r', long = "resume", value_name = "ID", num_args = 0..=1)]
    pub resume: Option<Option<String>>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// interactive first-run setup
    Init,
    /// daemon and devices summary
    Status,
    /// end-to-end health check
    Doctor,
    /// issue a pairing code (needs running daemon)
    Pair,
    /// pair this device against a remote daemon
    Connect {
        url: String,
        code: String,
        name: String,
        #[arg(long, value_name = "HEX")]
        fingerprint: Option<String>,
    },
    /// manage provider credentials
    Auth {
        #[command(subcommand)]
        cmd: AuthCmd,
    },
    /// manage daemon config
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// inspect or set the default model
    Model {
        #[command(subcommand)]
        cmd: ModelCmd,
    },
    /// browse discovered Agent Skills
    Skill {
        #[command(subcommand)]
        cmd: SkillCmd,
    },
    /// register MCP servers
    Mcp {
        #[command(subcommand)]
        cmd: McpCmd,
    },
    /// manage paired devices
    Device {
        #[command(subcommand)]
        cmd: DeviceCmd,
    },
    /// manage workspace trust grants
    Workspace {
        #[command(subcommand)]
        cmd: WorkspaceCmd,
    },
    /// inspect or migrate the local database
    Db {
        #[command(subcommand)]
        cmd: DbCmd,
    },
    /// inspect protocol versions and detect drift against the manifest
    Protocol {
        #[command(subcommand)]
        cmd: ProtocolCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum AuthCmd {
    /// inline picker — api key or Codex OAuth
    Login { provider: Option<String> },
    /// store a provider api key (non-interactive)
    Add {
        provider: String,
        key: String,
        label: Option<String>,
    },
    /// list stored credentials (keys masked)
    #[command(visible_alias = "ls")]
    List,
    /// delete a credential by id
    #[command(visible_alias = "rm")]
    Remove { id: String },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    #[command(visible_alias = "ls")]
    List,
    Get {
        key: String,
    },
    Set {
        key: String,
        value: String,
    },
    #[command(visible_alias = "rm")]
    Remove {
        key: String,
    },
    /// describe a config key — default, effect, summary, example
    Explain {
        /// omit to list every documented key
        key: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ModelCmd {
    #[command(visible_alias = "ls")]
    List,
    Set {
        id: String,
    },
    /// show default_model (no arg) or model metadata
    Get {
        id: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum SkillCmd {
    #[command(visible_alias = "ls")]
    List,
    Get {
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum McpCmd {
    #[command(visible_alias = "ls")]
    List,
    /// register an MCP server: harness mcp add NAME -- COMMAND [ARGS…]
    Add {
        name: String,
        command: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    #[command(visible_alias = "rm")]
    Remove { name: String },
}

#[derive(Subcommand, Debug)]
pub enum DeviceCmd {
    #[command(visible_alias = "ls")]
    List,
    #[command(visible_alias = "rm")]
    Remove { id: String },
}

// IMPLEMENTS: D-205
#[derive(Subcommand, Debug)]
pub enum WorkspaceCmd {
    /// grant trust for a directory (defaults to cwd)
    Trust { path: Option<std::path::PathBuf> },
    /// revoke trust for a directory (defaults to cwd)
    Untrust { path: Option<std::path::PathBuf> },
    /// show trust state for a directory (defaults to cwd)
    Status { path: Option<std::path::PathBuf> },
    /// list every trusted directory
    #[command(visible_alias = "ls")]
    List,
}

// IMPLEMENTS: D-316
#[derive(Subcommand, Debug)]
pub enum ProtocolCmd {
    /// list every protocol adapter the binary knows about
    #[command(visible_alias = "ls")]
    List,
    /// compare runtime adapters against the pinned manifest; non-zero exit on drift
    Check,
}

// IMPLEMENTS: D-080
#[derive(Subcommand, Debug)]
pub enum DbCmd {
    /// show schema version and pending migration count
    Status,
    /// apply pending migrations after backing the db up
    Migrate {
        /// list pending migrations without applying
        #[arg(long)]
        dry_run: bool,
        /// restore the previous backup instead of migrating
        #[arg(long)]
        restore: bool,
    },
}

#[must_use]
pub fn parse() -> Cli {
    Cli::parse()
}

pub async fn run(cli: Cli) -> Result<()> {
    style::set_plain(cli.plain);
    let Some(command) = cli.command else {
        bail!("no subcommand given (run with --help)");
    };
    match command {
        Command::Init => cmd_init().await,
        Command::Status => cmd_status(),
        Command::Doctor => cmd_doctor().await,
        Command::Pair => cmd_pair().await,
        Command::Connect {
            url,
            code,
            name,
            fingerprint,
        } => cmd_connect(&url, &code, &name, fingerprint.as_deref()).await,
        Command::Auth { cmd } => cmd_auth(cmd).await,
        Command::Config { cmd } => cmd_config(cmd).await,
        Command::Model { cmd } => cmd_model(cmd).await,
        Command::Skill { cmd } => cmd_skill(cmd).await,
        Command::Mcp { cmd } => cmd_mcp(cmd).await,
        Command::Device { cmd } => cmd_device(cmd).await,
        Command::Workspace { cmd } => cmd_workspace(cmd).await,
        Command::Db { cmd } => cmd_db(cmd).await,
        Command::Protocol { cmd } => cmd_protocol(cmd),
    }
}

// IMPLEMENTS: D-316
fn cmd_protocol(cmd: ProtocolCmd) -> Result<()> {
    let adapters = harness_proto_adapters::builtin_adapters();
    let refs: Vec<&dyn harness_proto_adapters::ProtocolAdapter> =
        adapters.iter().map(std::convert::AsRef::as_ref).collect();

    match cmd {
        ProtocolCmd::List => {
            style::section("Protocols");
            println!();
            let rows: Vec<Vec<String>> = refs
                .iter()
                .map(|a| {
                    let id = a.identity();
                    vec![id.name, id.current_version, id.schema_hash]
                })
                .collect();
            table::print(&["Name", "Version", "Schema hash"], &rows);
            Ok(())
        }
        ProtocolCmd::Check => {
            let manifest = harness_proto_adapters::manifest();
            let report = harness_proto_adapters::check(&manifest, &refs);
            let mut drift = false;
            style::section("Protocol drift check");
            println!();
            for (name, status) in &report {
                match status {
                    harness_proto_adapters::DriftStatus::InSync => {
                        style::success(format!("{name}: in sync"));
                    }
                    harness_proto_adapters::DriftStatus::VersionDrift { expected, actual } => {
                        drift = true;
                        style::failure(format!(
                            "{name}: version drift — expected {expected}, runtime {actual}"
                        ));
                    }
                    harness_proto_adapters::DriftStatus::SchemaDrift {
                        version,
                        expected_hash,
                        actual_hash,
                    } => {
                        drift = true;
                        style::failure(format!(
                            "{name}: schema drift at {version} — manifest {expected_hash}, runtime {actual_hash}"
                        ));
                    }
                }
            }
            println!();
            if drift {
                style::hint(
                    "auto-upgrade is intentionally disabled — bump the manifest after manual review",
                );
                bail!("protocol drift detected");
            }
            Ok(())
        }
    }
}

fn open_db() -> Result<(DataDir, Database, WriterHandle)> {
    let dir_path = data_dir();
    let dd = DataDir::init(&dir_path).context("init data dir")?;
    let db = Database::open(dd.db_path()).context("open db")?;
    let writer = Writer::spawn(&dd.db_path()).context("spawn writer")?;
    Ok((dd, db, writer))
}

const SETUP_PROVIDERS: &[&str] = &["anthropic", "openai", "google", "ollama"];

// IMPLEMENTS: D-041
async fn cmd_init() -> Result<()> {
    let theme = style::dialoguer_theme();
    style::section(&format!("harness init {VERSION}"));
    println!();
    let (dd, _db, writer) = open_db()?;
    style::kv("data dir", display_path(&dd.root), 12);
    println!();

    loop {
        let add = Confirm::with_theme(&theme)
            .with_prompt("Add a provider API key?")
            .default(false)
            .interact()?;
        if !add {
            break;
        }
        let idx = Select::with_theme(&theme)
            .with_prompt("Provider")
            .items(SETUP_PROVIDERS)
            .default(0)
            .interact()?;
        let provider = SETUP_PROVIDERS[idx];
        let key: String = Password::with_theme(&theme)
            .with_prompt(format!("{provider} API key"))
            .allow_empty_password(false)
            .interact()?;
        let label: String = Input::with_theme(&theme)
            .with_prompt("Label")
            .default("personal".to_string())
            .interact_text()?;
        add_credential(&writer, provider, &key, Some(&label)).await?;
        println!();
        style::success(format!("stored {provider} credential ({label})"));
        println!();
    }

    prompt_default_model(&theme, &dd, &writer).await?;
    prompt_workspace_trust(&theme, &dd, &writer).await?;

    let pair_now = Confirm::with_theme(&theme)
        .with_prompt("Pair a remote device?")
        .default(false)
        .interact()?;
    if pair_now {
        println!();
        match issue_pairing_code().await {
            Ok(info) => {
                let p = style::primary();
                println!(
                    "  {p}harness connect wss://<host>:{port} {code} <name>{p:#}",
                    port = info.port,
                    code = info.code
                );
            }
            Err(e) => style::failure(format!("could not reach daemon: {e}")),
        }
    }

    print_next_steps();
    Ok(())
}

// IMPLEMENTS: D-041
async fn prompt_default_model(
    theme: &dialoguer::theme::ColorfulTheme,
    dd: &DataDir,
    writer: &WriterHandle,
) -> Result<()> {
    let reader = rusqlite::Connection::open(dd.db_path()).context("open reader")?;
    if harness_storage::config::get(&reader, "default_model")
        .map_err(|e| anyhow!("{e}"))?
        .is_some()
    {
        return Ok(());
    }
    let registered: std::collections::BTreeSet<String> =
        harness_storage::credentials::list(&reader)
            .map_err(|e| anyhow!("{e}"))?
            .into_iter()
            .map(|c| c.provider)
            .collect();
    if registered.is_empty() {
        return Ok(());
    }
    let registry = ModelRegistry::with_builtins();
    let mut models: Vec<&harness_lifecycle::Model> = registry
        .iter()
        .filter(|m| registered.contains(&m.provider))
        .collect();
    if models.is_empty() {
        return Ok(());
    }
    models.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.id.cmp(&b.id)));
    let labels: Vec<String> = models
        .iter()
        .map(|m| format!("{:<10} {}", m.provider, m.id))
        .collect();
    let suggested = SETUP_DEFAULTS
        .iter()
        .find(|(p, _)| registered.contains(*p))
        .map(|(_, m)| *m);
    let default_idx = suggested
        .and_then(|s| models.iter().position(|m| m.id == s))
        .unwrap_or(0);
    println!();
    let idx = Select::with_theme(theme)
        .with_prompt("Default model")
        .items(&labels)
        .default(default_idx)
        .interact()?;
    let chosen = models[idx].id.clone();
    harness_storage::config::set(writer, "default_model".into(), chosen.clone())
        .await
        .map_err(|e| anyhow!("{e}"))?;
    style::success(format!("set default_model = {chosen}"));
    Ok(())
}

const SETUP_DEFAULTS: &[(&str, &str)] = &[
    ("anthropic", "claude-sonnet-4-6"),
    ("openai", "gpt-5.4"),
    ("google", "gemini-3.1-pro"),
];

// IMPLEMENTS: D-041
fn print_next_steps() {
    println!();
    style::section("Next steps");
    println!();
    let p = style::primary();
    println!("  1. start the daemon (in its own shell)");
    println!("       {p}harnessd{p:#}");
    println!();
    println!("  2. attach the tui");
    println!("       {p}harness{p:#}");
    println!();
    println!("  3. type a message and press enter");
    println!();
    style::hint("`harness doctor` runs an end-to-end health check if anything goes wrong");
}

// IMPLEMENTS: D-205
async fn prompt_workspace_trust(
    theme: &dialoguer::theme::ColorfulTheme,
    dd: &DataDir,
    writer: &WriterHandle,
) -> Result<()> {
    let cwd = std::env::current_dir().context("read current directory")?;
    let canonical = harness_storage::workspace_trust::canonicalize(&cwd);
    let display = display_path(&canonical);
    let reader = rusqlite::Connection::open(dd.db_path()).context("open reader")?;
    if harness_storage::workspace_trust::is_trusted(&reader, &canonical)
        .map_err(|e| anyhow!("{e}"))?
    {
        style::info(format!("workspace already trusted: {display}"));
        return Ok(());
    }
    drop(reader);
    let trust_now = Confirm::with_theme(theme)
        .with_prompt(format!(
            "Trust this workspace ({display})? Untrusted dirs cannot load AGENTS.md / CLAUDE.md / SKILL.md"
        ))
        .default(false)
        .interact()?;
    if trust_now {
        harness_storage::workspace_trust::trust(writer, canonical)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        style::success(format!("trusted {display}"));
    } else {
        style::hint("run `harness workspace trust` later to grant trust");
    }
    Ok(())
}

fn display_path(path: &std::path::Path) -> String {
    let raw = path.display().to_string();
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() {
        return raw.replacen(&home, "~", 1);
    }
    raw
}

struct PairingCodeInfo {
    code: String,
    fingerprint: String,
    port: u16,
}

async fn issue_pairing_code() -> Result<PairingCodeInfo> {
    let v = daemon_rpc::call("v1.auth.pair.new", None).await?;
    let code = v
        .get("code")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("daemon did not return a pairing code"))?
        .to_string();
    let fingerprint = v
        .get("fingerprint")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();
    let port = v
        .get("port")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u16::try_from(n).ok())
        .unwrap_or(8384);
    Ok(PairingCodeInfo {
        code,
        fingerprint,
        port,
    })
}

fn cmd_status() -> Result<()> {
    let (dd, _db, _writer) = open_db()?;
    let reg = ModelRegistry::with_builtins();
    let reader = rusqlite::Connection::open(dd.db_path()).context("open reader")?;

    let cred_count: i64 = reader
        .query_row("SELECT COUNT(*) FROM credentials", [], |r| r.get(0))
        .unwrap_or(0);

    let devices = pairing::list_devices(&reader).unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let dir_disp = dd.root.display().to_string().replacen(&home, "~", 1);

    style::section(&format!("harness {VERSION}"));
    println!();
    style::kv("data dir", dir_disp, 12);
    style::kv("models", reg.len(), 12);
    style::kv("credentials", cred_count, 12);
    style::kv("devices", devices.len(), 12);

    if !devices.is_empty() {
        println!();
        style::section("Devices");
        let max_name = devices
            .iter()
            .map(|d| d.name.chars().count())
            .max()
            .unwrap_or(0);
        let p = style::primary();
        for d in &devices {
            let pad = max_name.saturating_sub(d.name.chars().count());
            let key = fingerprint_short(&d.public_key.0);
            println!(
                "  {name}{padding}  {p}{key}{p:#}",
                name = d.name,
                padding = " ".repeat(pad),
            );
        }
    }
    Ok(())
}

fn fingerprint_short(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(13);
    for b in bytes.iter().take(6) {
        write!(s, "{b:02x}").unwrap();
    }
    s.push('…');
    s
}

async fn cmd_pair() -> Result<()> {
    let info = issue_pairing_code().await?;
    style::section("harness pair");
    println!();
    style::kv("code", &info.code, 12);
    style::kv("fingerprint", mask_fingerprint(&info.fingerprint), 12);
    style::kv("port", info.port, 12);

    let connect_cmd = format!(
        "harness connect wss://<host>:{port} {code} <name>",
        port = info.port,
        code = info.code
    );

    if style::supports_unicode() {
        println!();
        style::section("QR");
        let qr = render_qr(&connect_cmd);
        for line in qr.lines() {
            println!("  {line}");
        }
    }

    println!();
    style::section("Run on the other device");
    let p = style::primary();
    println!("  {p}{connect_cmd}{p:#}");
    println!();

    wait_for_pair(&info.code).await
}

const PAIR_TTL: std::time::Duration = std::time::Duration::from_secs(300);

async fn wait_for_pair(code: &str) -> Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};
    use std::time::{Duration, Instant};

    let pb = ProgressBar::new_spinner();
    let p = style::primary();
    let tmpl = format!("  {p}{{spinner}}{p:#} {{msg}}");
    pb.set_style(
        ProgressStyle::with_template(&tmpl)
            .expect("static template")
            .tick_strings(style::sym_spinner_frames()),
    );
    pb.enable_steady_tick(Duration::from_millis(100));
    let expires_at = Instant::now() + PAIR_TTL;
    pb.set_message(wait_message(remaining(expires_at)));

    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.tick().await;

    let rpc = daemon_rpc::call_with_timeout(
        "v1.auth.pair.await",
        Some(serde_json::json!({ "code": code })),
        PAIR_TTL + Duration::from_secs(15),
    );
    tokio::pin!(rpc);

    let outcome = loop {
        tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => break PairResult::UserCancelled,
            res = &mut rpc => break PairResult::Rpc(res),
            _ = tick.tick() => {
                pb.set_message(wait_message(remaining(expires_at)));
            }
        }
    };

    pb.finish_and_clear();
    match outcome {
        PairResult::Rpc(Ok(v)) => finish_pair_rpc(&v),
        PairResult::Rpc(Err(e)) => {
            style::failure(format!("daemon disconnected: {e}"));
            Err(e)
        }
        PairResult::UserCancelled => {
            daemon_rpc::call(
                "v1.auth.pair.cancel",
                Some(serde_json::json!({ "code": code })),
            )
            .await
            .ok();
            style::failure("cancelled — code invalidated");
            bail!("pairing cancelled")
        }
    }
}

enum PairResult {
    Rpc(Result<serde_json::Value>),
    UserCancelled,
}

fn finish_pair_rpc(v: &serde_json::Value) -> Result<()> {
    let status = v.get("status").and_then(|s| s.as_str()).unwrap_or("");
    match status {
        "connected" => {
            let name = v
                .get("device_name")
                .and_then(|s| s.as_str())
                .unwrap_or("device");
            let key = v
                .get("device_public_key")
                .and_then(|s| s.as_str())
                .unwrap_or("");
            style::success("paired");
            style::kv("device", name, 12);
            style::kv("key", mask_fingerprint(key), 12);
            Ok(())
        }
        "cancelled" => {
            style::failure("pairing cancelled on the daemon");
            bail!("pairing cancelled")
        }
        "expired" => {
            style::failure("pairing code expired");
            bail!("pairing code expired")
        }
        other => {
            style::failure(format!("daemon returned unknown status: {other}"));
            bail!("unexpected pair status: {other}")
        }
    }
}

fn remaining(expires_at: std::time::Instant) -> std::time::Duration {
    expires_at.saturating_duration_since(std::time::Instant::now())
}

fn wait_message(rem: std::time::Duration) -> String {
    let total = rem.as_secs();
    let m = total / 60;
    let s = total % 60;
    format!("waiting for a device to connect… ({m}:{s:02} left)")
}

fn mask_fingerprint(fp: &str) -> String {
    let chars: Vec<char> = fp.chars().collect();
    if chars.len() <= 14 {
        return fp.to_string();
    }
    let head: String = chars.iter().take(6).collect();
    let tail: String = chars
        .iter()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head}…{tail}")
}

fn render_qr(payload: &str) -> String {
    use qrcode::QrCode;
    use qrcode::render::unicode::Dense1x2;
    let Ok(code) = QrCode::new(payload.as_bytes()) else {
        return String::new();
    };
    code.render::<Dense1x2>()
        .dark_color(Dense1x2::Dark)
        .light_color(Dense1x2::Light)
        .build()
}

async fn cmd_connect(
    url: &str,
    code: &str,
    name: &str,
    expected_fingerprint: Option<&str>,
) -> Result<()> {
    let mut steps = progress::Steps::new(format!("connect to {url}"));
    let s_pair = steps.add("pair with daemon");
    let s_persist_fp = steps.add("persist fingerprint");
    let s_persist_id = steps.add("persist device id");

    let dd = DataDir::init(data_dir()).context("init data dir")?;
    let keyfile = dd.root.join("client.key");
    let (sk, pk) = load_or_generate_keypair(&keyfile)?;

    steps.start(s_pair);
    let outcome = match ws_client::pair(url, code, name, &sk, &pk, expected_fingerprint).await {
        Ok(o) => o,
        Err(e) => {
            steps.fail(s_pair, &e.to_string());
            return Err(e);
        }
    };
    steps.ok_message(
        s_pair,
        format!(
            "paired — device {}  fp {}",
            outcome.device_id,
            mask_fingerprint(&outcome.fingerprint)
        ),
    );

    let writer = Writer::spawn(&dd.db_path()).context("spawn writer")?;
    let _db = Database::open(dd.db_path()).context("open db")?;

    steps.start(s_persist_fp);
    if let Err(e) = harness_storage::config::set(
        &writer,
        format!("remote.{url}.fingerprint"),
        outcome.fingerprint.clone(),
    )
    .await
    {
        steps.fail(s_persist_fp, &e.to_string());
        return Err(anyhow::anyhow!("persist fingerprint: {e}"));
    }
    steps.ok(s_persist_fp);

    steps.start(s_persist_id);
    if let Err(e) = harness_storage::config::set(
        &writer,
        format!("remote.{url}.device_id"),
        outcome.device_id.clone(),
    )
    .await
    {
        steps.fail(s_persist_id, &e.to_string());
        return Err(anyhow::anyhow!("persist device id: {e}"));
    }
    steps.ok(s_persist_id);
    Ok(())
}

fn load_or_generate_keypair(keyfile: &std::path::Path) -> Result<(PrivateKey, PublicKey)> {
    if keyfile.exists() {
        let text = std::fs::read_to_string(keyfile)?;
        let bytes = hex_decode32(text.trim())
            .map_err(|e| anyhow!("{} is corrupt: {e}", keyfile.display()))?;
        let sk = PrivateKey::from_bytes(&bytes);
        let pk = sk.public();
        return Ok((sk, pk));
    }
    let (sk, pk) = generate_keypair();
    std::fs::write(keyfile, hex_encode(&sk.to_bytes()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(keyfile)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(keyfile, perms)?;
    }
    Ok((sk, pk))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        write!(s, "{b:02x}").unwrap();
    }
    s
}

fn hex_decode32(s: &str) -> Result<[u8; 32]> {
    if s.len() != 64 {
        bail!("expected 64 hex chars, got {}", s.len());
    }
    let mut out = [0u8; 32];
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = decode_nibble(s.as_bytes()[i * 2])?;
        let lo = decode_nibble(s.as_bytes()[i * 2 + 1])?;
        *slot = (hi << 4) | lo;
    }
    Ok(out)
}

fn decode_nibble(b: u8) -> Result<u8> {
    Ok(match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        other => bail!("invalid hex character: {}", other as char),
    })
}

async fn cmd_config(cmd: ConfigCmd) -> Result<()> {
    let (dd, _db, writer) = open_db()?;
    let reader = rusqlite::Connection::open(dd.db_path())?;
    match cmd {
        ConfigCmd::List => {
            let mut stmt = reader.prepare("SELECT key, value FROM config ORDER BY key")?;
            let rows: Vec<Vec<String>> = stmt
                .query_map([], |r| {
                    Ok(vec![r.get::<_, String>(0)?, r.get::<_, String>(1)?])
                })?
                .collect::<rusqlite::Result<_>>()?;
            if rows.is_empty() {
                style::info("no config entries");
            } else {
                table::print(&["Key", "Value"], &rows);
            }
        }
        ConfigCmd::Get { key } => {
            let value: Option<String> = reader
                .query_row(
                    "SELECT value FROM config WHERE key = ?1",
                    rusqlite::params![key],
                    |r| r.get(0),
                )
                .ok();
            if let Some(v) = value {
                println!("{v}");
            } else {
                bail!("no config entry for {key}");
            }
        }
        ConfigCmd::Set { key, value } => {
            let key_msg = key.clone();
            writer
                .execute(move |c| {
                    c.execute(
                        "INSERT INTO config (key, value) VALUES (?1, ?2)
                         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                        rusqlite::params![key, value],
                    )?;
                    Ok(())
                })
                .await
                .map_err(|e| anyhow!("{e}"))?;
            style::success(format!("set {key_msg}"));
        }
        ConfigCmd::Remove { key } => {
            let key_msg = key.clone();
            let n = writer
                .execute(move |c| {
                    Ok(c.execute("DELETE FROM config WHERE key = ?1", rusqlite::params![key])?)
                })
                .await
                .map_err(|e| anyhow!("{e}"))?;
            if n == 0 {
                bail!("no config entry for {key_msg}");
            }
            style::success(format!("removed {key_msg}"));
        }
        ConfigCmd::Explain { key } => match key {
            None => {
                style::section("Documented config keys");
                println!();
                let rows: Vec<Vec<String>> = config_explain::all()
                    .iter()
                    .map(|e| vec![e.key.to_string(), e.summary.to_string()])
                    .collect();
                table::print(&["Key", "Summary"], &rows);
                style::hint("run `harness config explain <key>` for details");
            }
            Some(k) => print_explain(&k),
        },
    }
    Ok(())
}

fn print_explain(key: &str) {
    if let Some(entry) = config_explain::lookup(key) {
        style::section(&format!("config {}", entry.key));
        println!();
        style::kv("default", entry.default, 14);
        style::kv("effect", entry.takes_effect, 14);
        style::kv("example", entry.example, 14);
        println!();
        println!("  {}", entry.summary);
        return;
    }
    if let Some((prefix, format, summary)) = config_explain::prefix_hint(key) {
        style::section(&format!("config namespace {prefix}*"));
        println!();
        style::kv("format", *format, 14);
        println!();
        println!("  {summary}");
        return;
    }
    style::failure(format!("no documentation for {key}"));
    style::hint("run `harness config explain` to list every documented key");
}

async fn cmd_auth(cmd: AuthCmd) -> Result<()> {
    if let AuthCmd::Login { provider } = &cmd {
        return auth_login::run(provider.as_deref()).await;
    }

    let (dd, _db, writer) = open_db()?;
    let reader = rusqlite::Connection::open(dd.db_path())?;

    match cmd {
        AuthCmd::Login { .. } => unreachable!("handled above"),
        AuthCmd::Add {
            provider,
            key,
            label,
        } => {
            add_credential(&writer, &provider, &key, label.as_deref()).await?;
            style::success(format!("stored {provider} credential"));
        }
        AuthCmd::List => {
            let mut stmt = reader.prepare(
                "SELECT id, provider, kind, value, label, created_at FROM credentials ORDER BY provider, created_at",
            )?;
            let rows: Vec<Vec<String>> = stmt
                .query_map([], |r| {
                    Ok(vec![
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        style::mask_key(&r.get::<_, String>(3)?),
                        r.get::<_, Option<String>>(4)?.unwrap_or_else(|| "-".into()),
                        short_id(&r.get::<_, String>(0)?),
                    ])
                })?
                .collect::<rusqlite::Result<_>>()?;
            if rows.is_empty() {
                style::info("no credentials");
            } else {
                style::section("Credentials");
                println!();
                table::print(&["Provider", "Kind", "Key", "Label", "ID"], &rows);
            }
        }
        AuthCmd::Remove { id } => {
            let id_owned = id.clone();
            let n = writer
                .execute(move |c| {
                    Ok(c.execute(
                        "DELETE FROM credentials WHERE id = ?1",
                        rusqlite::params![id_owned],
                    )?)
                })
                .await
                .map_err(|e| anyhow!("{e}"))?;
            if n == 0 {
                bail!("no credential with that id");
            }
            style::success("removed");
        }
    }
    Ok(())
}

async fn add_credential(
    writer: &WriterHandle,
    provider: &str,
    key: &str,
    label: Option<&str>,
) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let provider = provider.to_string();
    let key = key.to_string();
    let label = label.map(str::to_string);
    let ts = harness_core::now().as_millis();
    writer
        .execute(move |c| {
            c.execute(
                "INSERT INTO credentials (id, provider, kind, value, label, created_at)
                 VALUES (?1, ?2, 'api_key', ?3, ?4, ?5)",
                rusqlite::params![id, provider, key, label, ts],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow!("{e}"))?;
    Ok(())
}

fn short_id(uuid: &str) -> String {
    uuid.chars().take(8).collect()
}

async fn cmd_device(cmd: DeviceCmd) -> Result<()> {
    let (dd, _db, writer) = open_db()?;
    let reader = rusqlite::Connection::open(dd.db_path())?;
    match cmd {
        DeviceCmd::List => {
            let devices: Vec<DeviceRecord> =
                pairing::list_devices(&reader).map_err(|e| anyhow!("{e}"))?;
            if devices.is_empty() {
                style::info("no devices");
                return Ok(());
            }
            style::section("Devices");
            println!();
            let rows: Vec<Vec<String>> = devices
                .iter()
                .map(|d| {
                    vec![
                        short_id(&d.id),
                        d.name.clone(),
                        fingerprint_short(&d.public_key.0),
                    ]
                })
                .collect();
            table::print(&["ID", "Name", "Key"], &rows);
        }
        DeviceCmd::Remove { id } => {
            if pairing::revoke_device(&writer, id)
                .await
                .map_err(|e| anyhow!("{e}"))?
            {
                style::success("removed");
            } else {
                bail!("no device with that id");
            }
        }
    }
    Ok(())
}

// IMPLEMENTS: D-205
async fn cmd_workspace(cmd: WorkspaceCmd) -> Result<()> {
    let (dd, _db, writer) = open_db()?;
    let reader = rusqlite::Connection::open(dd.db_path())?;
    match cmd {
        WorkspaceCmd::Trust { path } => {
            let target = path.map_or_else(std::env::current_dir, Ok)?;
            let canonical = harness_storage::workspace_trust::canonicalize(&target);
            harness_storage::workspace_trust::trust(&writer, canonical.clone())
                .await
                .map_err(|e| anyhow!("{e}"))?;
            style::success(format!("trusted {}", display_path(&canonical)));
        }
        WorkspaceCmd::Untrust { path } => {
            let target = path.map_or_else(std::env::current_dir, Ok)?;
            let canonical = harness_storage::workspace_trust::canonicalize(&target);
            let removed = harness_storage::workspace_trust::untrust(&writer, canonical.clone())
                .await
                .map_err(|e| anyhow!("{e}"))?;
            if removed {
                style::success(format!("revoked {}", display_path(&canonical)));
            } else {
                bail!("not in the trust list: {}", display_path(&canonical));
            }
        }
        WorkspaceCmd::Status { path } => {
            let target = path.map_or_else(std::env::current_dir, Ok)?;
            let canonical = harness_storage::workspace_trust::canonicalize(&target);
            style::section(&format!("workspace {}", display_path(&canonical)));
            println!();
            let trusted = harness_storage::workspace_trust::is_trusted(&reader, &canonical)
                .map_err(|e| anyhow!("{e}"))?;
            style::kv("trusted", if trusted { "yes" } else { "no" }, 12);
        }
        WorkspaceCmd::List => {
            let entries =
                harness_storage::workspace_trust::list(&reader).map_err(|e| anyhow!("{e}"))?;
            if entries.is_empty() {
                style::info("no trusted workspaces");
                style::hint("run `harness workspace trust` in a directory to grant trust");
                return Ok(());
            }
            style::section("Trusted workspaces");
            println!();
            let rows: Vec<Vec<String>> = entries
                .iter()
                .map(|e| {
                    vec![
                        display_path(std::path::Path::new(&e.path)),
                        if e.trusted { "yes".into() } else { "no".into() },
                    ]
                })
                .collect();
            table::print(&["Path", "Trusted"], &rows);
        }
    }
    Ok(())
}

// IMPLEMENTS: D-080
async fn cmd_db(cmd: DbCmd) -> Result<()> {
    let dd = DataDir::init(data_dir()).context("init data dir")?;
    let db_path = dd.db_path();
    match cmd {
        DbCmd::Status => db_status(&db_path),
        DbCmd::Migrate { dry_run, restore } => db_migrate(&db_path, dry_run, restore).await,
    }
}

fn backup_path_for(db: &std::path::Path) -> std::path::PathBuf {
    db.with_extension("backup")
}

fn db_status(db_path: &std::path::Path) -> Result<()> {
    let mut conn = rusqlite::Connection::open(db_path).context("open db")?;
    harness_storage::db::configure(&mut conn).context("configure db")?;
    let migrations = harness_storage::migrations::all();
    let current: usize = migrations
        .current_version(&conn)
        .map_err(|e| anyhow!("{e}"))?
        .into();
    let pending = migrations
        .pending_migrations(&conn)
        .map_err(|e| anyhow!("{e}"))?;
    let target = current + usize::try_from(pending.max(0)).unwrap_or(0);
    let backup = backup_path_for(db_path);

    style::section("Database");
    println!();
    style::kv("path", display_path(db_path), 12);
    style::kv("current", current, 12);
    style::kv("target", target, 12);
    style::kv("pending", pending, 12);
    if backup.exists() {
        style::kv("backup", display_path(&backup), 12);
        style::hint(
            "a backup exists from a previous migrate — `harness db migrate --restore` to roll back",
        );
    }
    Ok(())
}

async fn db_migrate(db_path: &std::path::Path, dry_run: bool, restore: bool) -> Result<()> {
    let backup = backup_path_for(db_path);

    if restore {
        if !backup.exists() {
            bail!("no backup at {}", display_path(&backup));
        }
        std::fs::rename(&backup, db_path).context("restore backup")?;
        style::success(format!("restored from {}", display_path(&backup)));
        return Ok(());
    }

    if backup.exists() {
        let theme = style::dialoguer_theme();
        let restore_now = Confirm::with_theme(&theme)
            .with_prompt(format!(
                "stale backup at {} — previous migrate may have crashed; restore it?",
                display_path(&backup)
            ))
            .default(true)
            .interact()?;
        if restore_now {
            std::fs::rename(&backup, db_path).context("restore backup")?;
            style::success("restored");
            return Ok(());
        }
        std::fs::remove_file(&backup).context("remove stale backup")?;
        style::hint("removed stale backup");
    }

    let mut conn = rusqlite::Connection::open(db_path).context("open db")?;
    harness_storage::db::configure(&mut conn).context("configure db")?;
    let migrations = harness_storage::migrations::all();
    let current: usize = migrations
        .current_version(&conn)
        .map_err(|e| anyhow!("{e}"))?
        .into();
    let pending = migrations
        .pending_migrations(&conn)
        .map_err(|e| anyhow!("{e}"))?;

    style::section("Database migrations");
    println!();
    style::kv("path", display_path(db_path), 12);
    style::kv("current", current, 12);
    style::kv("pending", pending, 12);

    if pending <= 0 {
        println!();
        style::info("nothing to do");
        return Ok(());
    }

    if dry_run {
        println!();
        style::hint("dry-run: nothing applied");
        return Ok(());
    }

    println!();
    style::section("Backing up");
    harness_storage::backup::run(&conn, &backup).map_err(|e| anyhow!("{e}"))?;
    style::success(format!("backup at {}", display_path(&backup)));
    drop(conn);

    let mut conn = rusqlite::Connection::open(db_path).context("reopen db for migration")?;
    harness_storage::db::configure(&mut conn).context("configure db")?;
    if let Err(e) = migrations.to_latest(&mut conn) {
        style::failure(format!("migration failed: {e}"));
        style::hint(format!(
            "rerun with `--restore` to roll back to backup at {}",
            display_path(&backup)
        ));
        bail!("migration failed: {e}");
    }

    let target: usize = migrations
        .current_version(&conn)
        .map_err(|e| anyhow!("{e}"))?
        .into();
    drop(conn);
    style::success(format!("migrated to version {target}"));

    // Backfill events from messages / tool_calls (idempotent).
    let writer = Writer::spawn(db_path).context("spawn writer")?;
    let stats = writer
        .with_tx(|tx| {
            let s = harness_storage::events::backfill(tx)?;
            Ok(s)
        })
        .await
        .map_err(|e| anyhow!("{e}"))?;
    if stats.messages > 0 || stats.tool_calls > 0 {
        style::success(format!(
            "backfilled events — messages: {}  tool_calls: {}",
            stats.messages, stats.tool_calls
        ));
    } else {
        style::info("events table already in sync");
    }

    std::fs::remove_file(&backup).context("remove backup after success")?;
    Ok(())
}

async fn cmd_model(cmd: ModelCmd) -> Result<()> {
    match cmd {
        ModelCmd::List => {
            let reg = ModelRegistry::with_builtins();
            let mut models: Vec<_> = reg.iter().collect();
            models.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.id.cmp(&b.id)));
            let rows: Vec<Vec<String>> = models
                .iter()
                .map(|m| {
                    vec![
                        m.id.clone(),
                        m.provider.clone(),
                        m.context_window.to_string(),
                    ]
                })
                .collect();
            style::section("Models");
            println!();
            table::print(&["ID", "Provider", "Context"], &rows);
            Ok(())
        }
        ModelCmd::Set { id } => {
            daemon_rpc::call(
                "v1.config.set",
                Some(serde_json::json!({ "key": "default_model", "value": &id })),
            )
            .await?;
            style::success(format!("set default_model to {id}"));
            Ok(())
        }
        ModelCmd::Get { id: None } => {
            let v = daemon_rpc::call(
                "v1.config.get",
                Some(serde_json::json!({ "key": "default_model" })),
            )
            .await?;
            let current = v.get("value").and_then(|x| x.as_str()).unwrap_or("(unset)");
            style::kv("default_model", current, 14);
            Ok(())
        }
        ModelCmd::Get { id: Some(id) } => {
            let reg = ModelRegistry::with_builtins();
            let Some(m) = reg.iter().find(|m| m.id == id) else {
                bail!("unknown model: {id}");
            };
            style::section(&format!("Model {}", m.id));
            println!();
            style::kv("provider", &m.provider, 10);
            style::kv("context", m.context_window, 10);
            Ok(())
        }
    }
}

async fn cmd_skill(cmd: SkillCmd) -> Result<()> {
    match cmd {
        SkillCmd::List => {
            let v = daemon_rpc::call("v1.skill.list", None).await?;
            let skills = v
                .get("skills")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            if skills.is_empty() {
                style::info("no skills discovered");
                style::hint(
                    "scan paths: <cwd>/.harness/skills, <cwd>/.agents/skills, <cwd>/.claude/skills",
                );
                style::hint(
                    "             ~/.harness/skills,      ~/.agents/skills,      ~/.claude/skills",
                );
                return Ok(());
            }
            let rows: Vec<Vec<String>> = skills
                .iter()
                .map(|s| {
                    vec![
                        s.get("name")
                            .and_then(|x| x.as_str())
                            .unwrap_or("?")
                            .to_string(),
                        s.get("description")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        s.get("location")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                    ]
                })
                .collect();
            style::section("Skills");
            println!();
            table::print(&["Name", "Description", "Location"], &rows);
            Ok(())
        }
        SkillCmd::Get { name } => {
            let v = daemon_rpc::call(
                "v1.skill.activate",
                Some(serde_json::json!({ "name": &name })),
            )
            .await?;
            let dir = v.get("directory").and_then(|x| x.as_str()).unwrap_or("");
            let body = v.get("body").and_then(|x| x.as_str()).unwrap_or("");
            let resources = v
                .get("resources")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            style::section(&format!("Skill {name}"));
            println!();
            style::kv("dir", dir, 10);
            if !resources.is_empty() {
                println!();
                style::section("Resources");
                for r in resources {
                    if let Some(s) = r.as_str() {
                        let d = style::dim();
                        println!("  {d}{s}{d:#}");
                    }
                }
            }
            println!();
            println!("{body}");
            Ok(())
        }
    }
}

async fn cmd_mcp(cmd: McpCmd) -> Result<()> {
    match cmd {
        McpCmd::List => {
            let v = daemon_rpc::call("v1.mcp.list", None).await?;
            let servers = v
                .get("servers")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            if servers.is_empty() {
                style::info("no MCP servers registered");
                style::hint("add one with `harness mcp add NAME -- COMMAND`");
                return Ok(());
            }
            let rows: Vec<Vec<String>> = servers
                .iter()
                .map(|s| {
                    vec![
                        s.get("name")
                            .and_then(|x| x.as_str())
                            .unwrap_or("?")
                            .to_string(),
                        s.get("command")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        s.get("args")
                            .and_then(|x| x.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .unwrap_or_default(),
                    ]
                })
                .collect();
            style::section("MCP servers");
            println!();
            table::print(&["Name", "Command", "Args"], &rows);
            Ok(())
        }
        McpCmd::Add {
            name,
            command,
            args,
        } => {
            let mut steps = progress::Steps::new(format!("register mcp `{name}`"));
            let s_register = steps.add("contact daemon and start server");
            steps.start(s_register);
            let v = match daemon_rpc::call(
                "v1.mcp.add",
                Some(serde_json::json!({
                    "name": &name,
                    "command": command,
                    "args": args,
                })),
            )
            .await
            {
                Ok(v) => v,
                Err(e) => {
                    steps.fail(s_register, &e.to_string());
                    return Err(e);
                }
            };
            let added = v.get("added").and_then(|x| x.as_str()).unwrap_or(&name);
            steps.ok_message(s_register, format!("registered {added}"));
            Ok(())
        }
        McpCmd::Remove { name } => {
            daemon_rpc::call("v1.mcp.remove", Some(serde_json::json!({ "name": &name }))).await?;
            style::success(format!("removed {name}"));
            Ok(())
        }
    }
}

async fn cmd_doctor() -> Result<()> {
    let mut steps = progress::Steps::new("harness doctor");
    let s_socket = steps.add(format!(
        "daemon socket ({})",
        data_dir().join("harness.sock").display()
    ));
    let s_proto = steps.add("protocol negotiate");
    let s_creds = steps.add("credentials registered");
    let s_skills = steps.add("skills discovered");
    let s_mcp = steps.add("mcp servers");

    steps.start(s_socket);
    let ping = match daemon_rpc::call("ping", None).await {
        Ok(v) => v,
        Err(e) => {
            steps.fail(s_socket, &e.to_string());
            println!();
            style::hint("start the daemon with `harnessd`");
            std::process::exit(1);
        }
    };
    let version = ping.get("version").and_then(|v| v.as_str()).unwrap_or("?");
    steps.ok_message(s_socket, format!("daemon socket  v{version}"));

    let mut any_failed = false;
    let mut hints: Vec<&str> = Vec::new();

    steps.start(s_proto);
    match daemon_rpc::call(
        "negotiate",
        Some(serde_json::json!({ "client_versions": [1] })),
    )
    .await
    {
        Ok(v) => {
            let sel = v
                .get("selected")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            if sel > 0 {
                steps.ok_message(s_proto, format!("protocol  v{sel}"));
            } else {
                any_failed = true;
                steps.fail(s_proto, "no version selected");
            }
        }
        Err(e) => {
            any_failed = true;
            steps.fail(s_proto, &e.to_string());
        }
    }

    steps.start(s_creds);
    match daemon_rpc::call("v1.auth.credentials.list", None).await {
        Ok(v) => {
            let count = v
                .get("credentials")
                .and_then(|x| x.as_array())
                .map_or(0, Vec::len);
            if count > 0 {
                steps.ok_message(s_creds, format!("credentials  {count} stored"));
            } else {
                any_failed = true;
                steps.fail(s_creds, "none stored");
                hints.push("run `harness auth login` to sign in a provider");
            }
        }
        Err(e) => {
            any_failed = true;
            steps.fail(s_creds, &e.to_string());
        }
    }

    steps.start(s_skills);
    match daemon_rpc::call("v1.skill.list", None).await {
        Ok(v) => {
            let count = v
                .get("skills")
                .and_then(|x| x.as_array())
                .map_or(0, Vec::len);
            steps.ok_message(s_skills, format!("skills  {count} discovered"));
        }
        Err(e) => {
            any_failed = true;
            steps.fail(s_skills, &e.to_string());
        }
    }

    steps.start(s_mcp);
    match daemon_rpc::call("v1.mcp.list", None).await {
        Ok(v) => {
            let count = v
                .get("servers")
                .and_then(|x| x.as_array())
                .map_or(0, Vec::len);
            steps.ok_message(s_mcp, format!("mcp servers  {count} registered"));
        }
        Err(e) => {
            any_failed = true;
            steps.fail(s_mcp, &e.to_string());
        }
    }

    println!();
    if any_failed {
        for h in &hints {
            style::hint(h);
        }
        if !hints.is_empty() {
            println!();
        }
        let r = style::err();
        println!("{r}1 or more checks failed{r:#}");
        std::process::exit(1);
    } else {
        let g = style::green();
        println!("{g}all checks passed{g:#}");
    }
    Ok(())
}
