mod auth_login;
mod daemon_rpc;
mod ws_client;

use anyhow::{Context, Result, anyhow, bail};
use harness_auth::pairing::{self, DeviceRecord};
use harness_auth::{PrivateKey, PublicKey, generate_keypair};
use harness_lifecycle::{DataDir, ModelRegistry, data_dir};
use harness_storage::{Database, Writer, WriterHandle};
use std::io::{self, Write};

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const SUBCOMMANDS: &[&str] = &[
    "setup", "status", "pair", "connect", "config", "auth", "device", "model", "skill", "mcp",
    "doctor", "help",
];

#[must_use]
pub fn is_subcommand(arg: &str) -> bool {
    SUBCOMMANDS.contains(&arg)
}

pub async fn run(args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        print_usage();
        return Ok(());
    }

    match args[0].as_str() {
        "-h" | "--help" | "help" => {
            print_usage();
            Ok(())
        }
        "-V" | "--version" | "version" => {
            println!("{VERSION}");
            Ok(())
        }
        "setup" => cmd_setup().await,
        "status" => cmd_status(),
        "pair" => cmd_pair().await,
        "connect" => cmd_connect(&args[1..]).await,
        "config" => cmd_config(&normalize_alias(&args[1..])).await,
        "auth" => cmd_auth(&normalize_alias(&args[1..])).await,
        "device" => cmd_device(&normalize_alias(&args[1..])).await,
        "model" => cmd_model(&normalize_alias(&args[1..])).await,
        "skill" => cmd_skill(&normalize_alias(&args[1..])).await,
        "mcp" => cmd_mcp(&normalize_alias(&args[1..])).await,
        "doctor" => cmd_doctor().await,
        other => {
            eprintln!("unknown command: {other}");
            print_usage();
            std::process::exit(2);
        }
    }
}

fn normalize_alias(args: &[String]) -> Vec<String> {
    let mut out: Vec<String> = args.to_vec();
    if let Some(first) = out.first_mut() {
        let canonical = match first.as_str() {
            "ls" => Some("list"),
            "rm" | "delete" | "del" => Some("remove"),
            "show" => Some("get"),
            _ => None,
        };
        if let Some(c) = canonical {
            *first = c.to_string();
        }
    }
    out
}

fn print_usage() {
    eprintln!(
        "harness-cli {VERSION}\n\n\
         USAGE:\n    \
             harness                                open the TUI\n    \
             harness <COMMAND> [ARGS]               run a subcommand\n\n\
         SESSION FLAGS:\n    \
             -c, --continue                         resume cwd's latest session\n    \
             -r, --resume [ID]                      resume session by id (picker when omitted)\n\n\
         COMMANDS:\n    \
             setup                                  interactive first-run setup\n    \
             status                                 show daemon + paired devices\n    \
             pair                                   issue a pairing code (needs running daemon)\n    \
             connect WSS_URL CODE NAME [--fingerprint HEX]\n                                                    pair THIS device against a remote daemon\n    \
             config list|get|set|unset KEY [VALUE]  manage config\n    \
             auth login [PROVIDER]                  inline picker — API key or Codex OAuth\n    \
             auth add PROVIDER KEY                  store a provider API key (non-interactive)\n    \
             auth list                              list stored credentials (keys masked)\n    \
             auth remove ID                         delete a credential by id (aliases: rm/delete/del)\n    \
             model list                             list known models (alias: ls)\n    \
             model use ID                           set default_model\n    \
             model current                          show current default\n    \
             skill list                             list discovered Agent Skills\n    \
             skill info NAME                        show a skill's description + path\n    \
             mcp list                               list registered MCP servers\n    \
             mcp add NAME -- COMMAND [ARGS…]        register an MCP server\n    \
             mcp remove NAME                        unregister an MCP server (alias: rm)\n    \
             device list                            list paired devices\n    \
             device revoke ID                       remove a device\n    \
             doctor                                 run end-to-end health checks\n"
    );
}

fn open_db() -> Result<(DataDir, Database, WriterHandle)> {
    let dir_path = data_dir();
    let dd = DataDir::init(&dir_path).context("init data dir")?;
    let db = Database::open(dd.db_path()).context("open db")?;
    let writer = Writer::spawn(&dd.db_path()).context("spawn writer")?;
    Ok((dd, db, writer))
}

async fn cmd_setup() -> Result<()> {
    println!("harness setup — {VERSION}\n");
    let (dd, _db, writer) = open_db()?;
    println!("data directory: {}", dd.root.display());

    loop {
        let add = prompt("Add a provider API key? [y/N] ")?;
        if !add.eq_ignore_ascii_case("y") {
            break;
        }
        let provider = prompt("provider (anthropic|openai|google|ollama): ")?;
        let key = prompt("api key: ")?;
        let label = prompt("label (default: personal): ")?;
        let label = if label.is_empty() {
            "personal".into()
        } else {
            label
        };
        add_credential(&writer, &provider, &key, Some(&label)).await?;
        println!("✓ stored {provider} credential '{label}'");
    }

    let pair_now = prompt("Pair a remote device now? [y/N] ")?;
    if pair_now.eq_ignore_ascii_case("y") {
        match issue_pairing_code().await {
            Ok(info) => {
                println!(
                    "\npairing code: {}\n  port: {}\n  fingerprint: {}\n",
                    info.code, info.port, info.fingerprint
                );
                println!(
                    "Run on the other device:\n    \
                     harness-cli connect wss://<host>:{port} {code} <device-name>\n",
                    port = info.port,
                    code = info.code
                );
            }
            Err(e) => eprintln!("could not reach daemon for pairing: {e}"),
        }
    }

    println!("\nsetup complete. Run `harness` to start the TUI or `harness-cli status`.");
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

fn prompt(label: &str) -> Result<String> {
    let mut stdout = io::stdout().lock();
    write!(stdout, "{label}")?;
    stdout.flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

fn cmd_status() -> Result<()> {
    let (dd, db, _writer) = open_db()?;
    let reg = ModelRegistry::with_builtins();
    let reader = rusqlite::Connection::open(dd.db_path()).context("open reader")?;

    let cred_count: i64 = reader
        .query_row("SELECT COUNT(*) FROM credentials", [], |r| r.get(0))
        .unwrap_or(0);

    let devices = pairing::list_devices(&reader).unwrap_or_default();

    println!("harness-cli {VERSION}");
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
         harness-cli connect wss://<host>:{port} {code} <device-name>\n",
        port = info.port,
        code = info.code,
    );
    Ok(())
}

async fn cmd_connect(args: &[String]) -> Result<()> {
    if args.len() < 3 {
        bail!("usage: harness-cli connect WSS_URL CODE DEVICE_NAME [--fingerprint HEX]");
    }
    let url = args[0].clone();
    let code = args[1].clone();
    let name = args[2].clone();
    let expected_fingerprint = args
        .iter()
        .position(|a| a == "--fingerprint")
        .and_then(|i| args.get(i + 1).cloned());

    let dd = DataDir::init(data_dir()).context("init data dir")?;
    let keyfile = dd.root.join("client.key");
    let (sk, pk) = load_or_generate_keypair(&keyfile)?;

    println!("connecting to {url} …");
    let outcome = ws_client::pair(
        &url,
        &code,
        &name,
        &sk,
        &pk,
        expected_fingerprint.as_deref(),
    )
    .await?;
    println!("✓ paired. device id: {}", outcome.device_id);
    println!("  fingerprint: {}", outcome.fingerprint);

    let writer = Writer::spawn(&dd.db_path()).context("spawn writer")?;
    let _db = Database::open(dd.db_path()).context("open db")?;
    harness_storage::config::set(
        &writer,
        format!("remote.{url}.fingerprint"),
        outcome.fingerprint.clone(),
    )
    .await
    .context("persist fingerprint")?;
    harness_storage::config::set(
        &writer,
        format!("remote.{url}.device_id"),
        outcome.device_id.clone(),
    )
    .await
    .context("persist device id")?;
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

async fn cmd_config(args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("config needs a subcommand: list | set | get");
    }
    let (dd, _db, writer) = open_db()?;
    let reader = rusqlite::Connection::open(dd.db_path())?;
    match args[0].as_str() {
        "list" => {
            let mut stmt = reader.prepare("SELECT key, value FROM config ORDER BY key")?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for r in rows {
                let (k, v) = r?;
                println!("{k} = {v}");
            }
        }
        "set" => {
            if args.len() < 3 {
                bail!("usage: config set KEY VALUE");
            }
            let key = args[1].clone();
            let value = args[2].clone();
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
        "get" => {
            if args.len() < 2 {
                bail!("usage: config get KEY");
            }
            let value: Option<String> = reader
                .query_row(
                    "SELECT value FROM config WHERE key = ?1",
                    rusqlite::params![args[1]],
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
        other => bail!("unknown config subcommand: {other}"),
    }
    Ok(())
}

async fn cmd_auth(args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("auth needs a subcommand: login | add | list | remove");
    }

    if args[0] == "login" {
        let preselected = args.get(1).map(String::as_str);
        return auth_login::run(preselected).await;
    }

    let (dd, _db, writer) = open_db()?;
    let reader = rusqlite::Connection::open(dd.db_path())?;

    match args[0].as_str() {
        "add" => {
            if args.len() < 3 {
                bail!("usage: auth add PROVIDER KEY [LABEL]");
            }
            let provider = &args[1];
            let key = &args[2];
            let label = args.get(3).map(String::as_str);
            add_credential(&writer, provider, key, label).await?;
            println!("ok");
        }
        "list" => {
            let mut stmt = reader.prepare(
                "SELECT id, provider, kind, value, label, created_at FROM credentials ORDER BY provider, created_at",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })?;
            for r in rows {
                let (id, provider, kind, value, label) = r?;
                println!(
                    "{provider} {kind} {}  {}  id={id}",
                    mask(&value),
                    label.unwrap_or_else(|| "-".into())
                );
            }
        }
        "remove" => {
            if args.len() < 2 {
                bail!("usage: auth remove ID");
            }
            let id = args[1].clone();
            let n = writer
                .execute(move |c| {
                    Ok(c.execute(
                        "DELETE FROM credentials WHERE id = ?1",
                        rusqlite::params![id],
                    )?)
                })
                .await
                .map_err(|e| anyhow!("{e}"))?;
            if n == 0 {
                bail!("no credential with that id");
            }
            println!("removed");
        }
        other => bail!("unknown auth subcommand: {other}"),
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

async fn cmd_device(args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("device needs a subcommand: list | revoke");
    }
    let (dd, _db, writer) = open_db()?;
    let reader = rusqlite::Connection::open(dd.db_path())?;
    match args[0].as_str() {
        "list" => {
            let devices: Vec<DeviceRecord> =
                pairing::list_devices(&reader).map_err(|e| anyhow!("{e}"))?;
            if devices.is_empty() {
                println!("(no devices)");
                return Ok(());
            }
            for d in devices {
                use std::fmt::Write;
                let mut pk_hex = String::with_capacity(16);
                for b in d.public_key.0.iter().take(8) {
                    write!(pk_hex, "{b:02x}").unwrap();
                }
                println!("{}  {}  {pk_hex}…", d.id, d.name);
            }
        }
        "revoke" => {
            if args.len() < 2 {
                bail!("usage: device revoke ID");
            }
            let id = args[1].clone();
            if pairing::revoke_device(&writer, id)
                .await
                .map_err(|e| anyhow!("{e}"))?
            {
                println!("revoked");
            } else {
                bail!("no device with that id");
            }
        }
        other => bail!("unknown device subcommand: {other}"),
    }
    Ok(())
}

async fn cmd_model(args: &[String]) -> Result<()> {
    match args.first().map_or("list", String::as_str) {
        "list" => {
            let reg = ModelRegistry::with_builtins();
            let mut models: Vec<_> = reg.iter().collect();
            models.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.id.cmp(&b.id)));
            for m in models {
                println!(
                    "{}  ctx={}  provider={}",
                    m.id, m.context_window, m.provider
                );
            }
            Ok(())
        }
        "use" | "set" | "default" => {
            let Some(id) = args.get(1) else {
                bail!("usage: harness model use <model-id>");
            };
            daemon_rpc::call(
                "v1.config.set",
                Some(serde_json::json!({ "key": "default_model", "value": id })),
            )
            .await?;
            println!("default_model ← {id}");
            Ok(())
        }
        "current" | "get" => {
            let v = daemon_rpc::call(
                "v1.config.get",
                Some(serde_json::json!({ "key": "default_model" })),
            )
            .await?;
            let current = v.get("value").and_then(|x| x.as_str()).unwrap_or("(unset)");
            println!("default_model: {current}");
            Ok(())
        }
        other => bail!("unknown model subcommand: {other}"),
    }
}

async fn cmd_skill(args: &[String]) -> Result<()> {
    match args.first().map_or("list", String::as_str) {
        "list" => {
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
            for s in skills {
                let name = s.get("name").and_then(|x| x.as_str()).unwrap_or("?");
                let desc = s.get("description").and_then(|x| x.as_str()).unwrap_or("");
                let loc = s.get("location").and_then(|x| x.as_str()).unwrap_or("");
                println!("{name:22}  {desc}");
                println!("{empty:22}  {loc}", empty = "");
            }
            Ok(())
        }
        "info" | "get" => {
            let Some(name) = args.get(1) else {
                bail!("usage: harness skill info <name>");
            };
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
        other => bail!("unknown skill subcommand: {other}"),
    }
}

async fn cmd_mcp(args: &[String]) -> Result<()> {
    match args.first().map_or("list", String::as_str) {
        "list" => {
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
            for s in servers {
                let name = s.get("name").and_then(|x| x.as_str()).unwrap_or("?");
                let cmd = s.get("command").and_then(|x| x.as_str()).unwrap_or("");
                let args = s
                    .get("args")
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default();
                println!("{name:16}  {cmd} {args}");
            }
            Ok(())
        }
        "add" => {
            let Some(name) = args.get(1) else {
                bail!("usage: harness mcp add NAME -- COMMAND [ARGS…]");
            };
            let sep = args
                .iter()
                .position(|a| a == "--")
                .ok_or_else(|| anyhow!("usage: harness mcp add NAME -- COMMAND [ARGS…]"))?;
            let rest = &args[sep + 1..];
            let Some(command) = rest.first() else {
                bail!("missing COMMAND after `--`");
            };
            let inner_args: Vec<String> = rest[1..].to_vec();
            let v = daemon_rpc::call(
                "v1.mcp.add",
                Some(serde_json::json!({
                    "name": name,
                    "command": command,
                    "args": inner_args,
                })),
            )
            .await?;
            let added = v.get("added").and_then(|x| x.as_str()).unwrap_or(name);
            println!("registered: {added}");
            Ok(())
        }
        "remove" => {
            let Some(name) = args.get(1) else {
                bail!("usage: harness mcp remove NAME");
            };
            daemon_rpc::call("v1.mcp.remove", Some(serde_json::json!({ "name": name }))).await?;
            println!("removed: {name}");
            Ok(())
        }
        other => bail!("unknown mcp subcommand: {other}"),
    }
}

async fn cmd_doctor() -> Result<()> {
    let mut any_failed = false;
    let mark = |ok: bool| if ok { "✓" } else { "✗" };

    let ping = daemon_rpc::call("ping", None).await;
    let daemon_ok = ping.is_ok();
    println!(
        "{} daemon socket ({})",
        mark(daemon_ok),
        data_dir().join("harness.sock").display()
    );
    if !daemon_ok {
        println!(
            "    start it first: `harnessd` (or launchctl load ~/Library/LaunchAgents/com.harness.plist)"
        );
        return Ok(());
    }
    let ping = ping?;
    let version = ping.get("version").and_then(|v| v.as_str()).unwrap_or("?");
    println!("{} daemon version: {version}", mark(true));

    let negotiate = daemon_rpc::call(
        "negotiate",
        Some(serde_json::json!({ "client_versions": [1] })),
    )
    .await;
    match &negotiate {
        Ok(v) => {
            let sel = v
                .get("selected")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            println!("{} protocol: v{sel}", mark(sel > 0));
        }
        Err(e) => {
            any_failed = true;
            println!("{} protocol negotiate failed: {e}", mark(false));
        }
    }

    match daemon_rpc::call("v1.auth.credentials.list", None).await {
        Ok(v) => {
            let count = v
                .get("credentials")
                .and_then(|x| x.as_array())
                .map_or(0, Vec::len);
            let ok = count > 0;
            any_failed |= !ok;
            println!("{} credentials: {count}", mark(ok));
            if !ok {
                println!("    add one: `harness auth login`");
            }
        }
        Err(e) => {
            any_failed = true;
            println!("{} credentials list failed: {e}", mark(false));
        }
    }

    match daemon_rpc::call("v1.skill.list", None).await {
        Ok(v) => {
            let count = v
                .get("skills")
                .and_then(|x| x.as_array())
                .map_or(0, Vec::len);
            println!("{} skills discovered: {count}", mark(true));
        }
        Err(e) => {
            any_failed = true;
            println!("{} skill.list failed: {e}", mark(false));
        }
    }

    match daemon_rpc::call("v1.mcp.list", None).await {
        Ok(v) => {
            let count = v
                .get("servers")
                .and_then(|x| x.as_array())
                .map_or(0, Vec::len);
            println!("{} MCP servers registered: {count}", mark(true));
        }
        Err(e) => {
            any_failed = true;
            println!("{} mcp.list failed: {e}", mark(false));
        }
    }

    if any_failed {
        std::process::exit(1);
    }
    Ok(())
}
