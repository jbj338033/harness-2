mod auth_login;
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
    version,
    about = "persistent, multi-agent AI coding daemon",
    arg_required_else_help = false,
    args_conflicts_with_subcommands = true,
    styles = style::clap_styles(),
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub session: SessionFlags,
}

#[derive(clap::Args, Debug, Default)]
pub struct SessionFlags {
    /// resume cwd's latest session
    #[arg(short = 'c', long = "continue")]
    pub continue_session: bool,

    /// resume session by ID; omit to pick interactively
    #[arg(short = 'r', long = "resume", value_name = "ID", num_args = 0..=1)]
    pub resume: Option<Option<String>>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// interactive first-run setup
    Setup,
    /// daemon + paired devices summary
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
}

#[derive(Subcommand, Debug)]
pub enum AuthCmd {
    /// inline picker — API key or Codex OAuth
    Login { provider: Option<String> },
    /// store a provider API key (non-interactive)
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

#[must_use]
pub fn parse() -> Cli {
    Cli::parse()
}

pub async fn run(cli: Cli) -> Result<()> {
    let Some(command) = cli.command else {
        bail!("no subcommand given (run with --help)");
    };
    match command {
        Command::Setup => cmd_setup().await,
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

async fn cmd_setup() -> Result<()> {
    let theme = style::dialoguer_theme();
    println!("harness setup — {VERSION}\n");
    let (dd, _db, writer) = open_db()?;
    println!("data directory: {}\n", dd.root.display());

    loop {
        let add = Confirm::with_theme(&theme)
            .with_prompt("Add a provider API key?")
            .default(false)
            .interact()?;
        if !add {
            break;
        }
        let idx = Select::with_theme(&theme)
            .with_prompt("provider")
            .items(SETUP_PROVIDERS)
            .default(0)
            .interact()?;
        let provider = SETUP_PROVIDERS[idx];
        let key: String = Password::with_theme(&theme)
            .with_prompt(format!("{provider} API key"))
            .allow_empty_password(false)
            .interact()?;
        let label: String = Input::with_theme(&theme)
            .with_prompt("label")
            .default("personal".to_string())
            .interact_text()?;
        add_credential(&writer, provider, &key, Some(&label)).await?;
        println!("  ✓ stored {provider} credential '{label}'\n");
    }

    let pair_now = Confirm::with_theme(&theme)
        .with_prompt("Pair a remote device now?")
        .default(false)
        .interact()?;
    if pair_now {
        match issue_pairing_code().await {
            Ok(info) => {
                println!(
                    "\npairing code: {}\n  port: {}\n  fingerprint: {}\n",
                    info.code, info.port, info.fingerprint
                );
                println!(
                    "Run on the other device:\n    \
                     harness connect wss://<host>:{port} {code} <device-name>\n",
                    port = info.port,
                    code = info.code
                );
            }
            Err(e) => eprintln!("could not reach daemon for pairing: {e}"),
        }
    }

    println!("\nsetup complete. Run `harness` to start the TUI or `harness status`.");
    Ok(())
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
    let (dd, db, _writer) = open_db()?;
    let reg = ModelRegistry::with_builtins();
    let reader = rusqlite::Connection::open(dd.db_path()).context("open reader")?;

    let cred_count: i64 = reader
        .query_row("SELECT COUNT(*) FROM credentials", [], |r| r.get(0))
        .unwrap_or(0);

    let devices = pairing::list_devices(&reader).unwrap_or_default();

    println!("harness {VERSION}");
    println!("data dir:      {}", dd.root.display());
    println!("database:      {}", db.path);
    println!("models:        {}", reg.len());
    println!("credentials:   {cred_count}");
    println!("devices:       {}", devices.len());
    for d in devices {
        println!("  - {} ({})", d.name, d.id);
    }
    Ok(())
}

async fn cmd_pair() -> Result<()> {
    let info = issue_pairing_code().await?;
    println!("pairing code:  {}", info.code);
    println!("fingerprint:   {}", info.fingerprint);
    println!("port:          {}", info.port);
    println!();
    println!(
        "Run on the other device:\n    \
         harness connect wss://<host>:{port} {code} <device-name>\n",
        port = info.port,
        code = info.code,
    );
    Ok(())
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
            outcome.device_id, outcome.fingerprint
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
                println!("(no config entries)");
            } else {
                table::print(&["KEY", "VALUE"], &rows);
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
                eprintln!("(unset)");
                std::process::exit(1);
            }
        }
        ConfigCmd::Set { key, value } => {
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
            println!("ok");
        }
        ConfigCmd::Remove { key } => {
            let n = writer
                .execute(move |c| {
                    Ok(c.execute("DELETE FROM config WHERE key = ?1", rusqlite::params![key])?)
                })
                .await
                .map_err(|e| anyhow!("{e}"))?;
            if n == 0 {
                bail!("no config entry with key");
            }
            println!("removed");
        }
    }
    Ok(())
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
            println!("ok");
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
                        mask(&r.get::<_, String>(3)?),
                        r.get::<_, Option<String>>(4)?.unwrap_or_else(|| "-".into()),
                        r.get::<_, String>(0)?,
                    ])
                })?
                .collect::<rusqlite::Result<_>>()?;
            if rows.is_empty() {
                println!("(no credentials)");
            } else {
                table::print(&["PROVIDER", "KIND", "KEY", "LABEL", "ID"], &rows);
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
            println!("removed");
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

fn mask(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 8 {
        return "****".into();
    }
    let visible_start: String = chars.iter().take(4).collect();
    let mut tail: Vec<char> = chars.iter().rev().take(4).copied().collect();
    tail.reverse();
    let visible_end: String = tail.into_iter().collect();
    format!("{visible_start}…{visible_end}")
}

async fn cmd_device(cmd: DeviceCmd) -> Result<()> {
    let (dd, _db, writer) = open_db()?;
    let reader = rusqlite::Connection::open(dd.db_path())?;
    match cmd {
        DeviceCmd::List => {
            let devices: Vec<DeviceRecord> =
                pairing::list_devices(&reader).map_err(|e| anyhow!("{e}"))?;
            if devices.is_empty() {
                println!("(no devices)");
                return Ok(());
            }
            let rows: Vec<Vec<String>> = devices
                .iter()
                .map(|d| {
                    use std::fmt::Write;
                    let mut pk_hex = String::with_capacity(17);
                    for b in d.public_key.0.iter().take(8) {
                        write!(pk_hex, "{b:02x}").unwrap();
                    }
                    pk_hex.push('…');
                    vec![d.id.clone(), d.name.clone(), pk_hex]
                })
                .collect();
            table::print(&["ID", "NAME", "KEY"], &rows);
        }
        DeviceCmd::Remove { id } => {
            if pairing::revoke_device(&writer, id)
                .await
                .map_err(|e| anyhow!("{e}"))?
            {
                println!("removed");
            } else {
                bail!("no device with that id");
            }
        }
    }
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
            table::print(&["ID", "PROVIDER", "CONTEXT"], &rows);
            Ok(())
        }
        ModelCmd::Set { id } => {
            daemon_rpc::call(
                "v1.config.set",
                Some(serde_json::json!({ "key": "default_model", "value": id })),
            )
            .await?;
            println!("default_model ← {id}");
            Ok(())
        }
        ModelCmd::Get { id: None } => {
            let v = daemon_rpc::call(
                "v1.config.get",
                Some(serde_json::json!({ "key": "default_model" })),
            )
            .await?;
            let current = v.get("value").and_then(|x| x.as_str()).unwrap_or("(unset)");
            println!("default_model: {current}");
            Ok(())
        }
        ModelCmd::Get { id: Some(id) } => {
            let reg = ModelRegistry::with_builtins();
            let Some(m) = reg.iter().find(|m| m.id == id) else {
                bail!("unknown model: {id}");
            };
            println!("id:        {}", m.id);
            println!("provider:  {}", m.provider);
            println!("context:   {}", m.context_window);
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
                println!("no skills discovered. scan paths:");
                println!("  <cwd>/.harness/skills,  <cwd>/.agents/skills,  <cwd>/.claude/skills");
                println!("  ~/.harness/skills,      ~/.agents/skills,      ~/.claude/skills");
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
            table::print(&["NAME", "DESCRIPTION", "LOCATION"], &rows);
            Ok(())
        }
        SkillCmd::Get { name } => {
            let v = daemon_rpc::call(
                "v1.skill.activate",
                Some(serde_json::json!({ "name": name })),
            )
            .await?;
            let dir = v.get("directory").and_then(|x| x.as_str()).unwrap_or("");
            let body = v.get("body").and_then(|x| x.as_str()).unwrap_or("");
            let resources = v
                .get("resources")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            println!("skill: {name}");
            println!("dir:   {dir}");
            if !resources.is_empty() {
                println!("resources:");
                for r in resources {
                    if let Some(s) = r.as_str() {
                        println!("  - {s}");
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
                println!(
                    "no MCP servers registered. add one with `harness mcp add NAME -- COMMAND`."
                );
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
            table::print(&["NAME", "COMMAND", "ARGS"], &rows);
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
                    "name": name,
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
            steps.ok_message(s_register, format!("registered: {added}"));
            Ok(())
        }
        McpCmd::Remove { name } => {
            daemon_rpc::call("v1.mcp.remove", Some(serde_json::json!({ "name": name }))).await?;
            println!("removed: {name}");
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
            println!("    start the daemon: `harnessd`");
            std::process::exit(1);
        }
    };
    let version = ping.get("version").and_then(|v| v.as_str()).unwrap_or("?");
    steps.ok_message(s_socket, format!("daemon socket — v{version}"));

    let mut any_failed = false;

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
                steps.ok_message(s_proto, format!("protocol v{sel}"));
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
                steps.ok_message(s_creds, format!("credentials: {count}"));
            } else {
                any_failed = true;
                steps.fail(s_creds, "none — run `harness auth login`");
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
            steps.ok_message(s_skills, format!("skills: {count}"));
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
            steps.ok_message(s_mcp, format!("mcp servers: {count}"));
        }
        Err(e) => {
            any_failed = true;
            steps.fail(s_mcp, &e.to_string());
        }
    }

    if any_failed {
        std::process::exit(1);
    }
    Ok(())
}
