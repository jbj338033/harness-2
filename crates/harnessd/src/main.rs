mod approval_requester;
mod authenticator;
mod ollama;
mod providers;
mod rpc;
mod tools;

use crate::approval_requester::PendingApprovals;
use crate::authenticator::DeviceAuthenticator;
use crate::tools::RegistryInputs;
use harness_auth::pairing::PairingSession;
use harness_lifecycle::{DataDir, ModelRegistry, Shutdown, UpdateChecker, data_dir};
use harness_llm::ProviderPool;
use harness_mcp::Supervisor as McpSupervisor;
use harness_session::SessionBroadcaster;
use harness_storage::{Database, ReaderPool, Writer, WriterHandle, backup, config as cfg_store};
use harness_tools::Registry;
use harness_transport::{Authenticator, TlsMaterials, WsConfig, serve_unix, serve_ws};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::{MissedTickBehavior, interval};
use tracing::{debug, error, info, warn};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DAILY: Duration = Duration::from_secs(24 * 60 * 60);
const UPDATE_TICK: Duration = Duration::from_secs(3600);
const UPDATE_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
const DEFAULT_WS_PORT: u16 = 8384;
const DEFAULT_WS_HOST: &str = "0.0.0.0";
const DEFAULT_UPDATE_ENDPOINT: &str =
    "https://api.github.com/repos/jbj338033/harness/releases/latest";

pub struct Daemon {
    pub storage: StorageLayer,
    pub llm: LlmLayer,
    pub network: NetworkLayer,
    pub security: SecurityLayer,
    pub tools: ToolsLayer,
    pub shutdown: Shutdown,
}

pub struct StorageLayer {
    pub data_dir: DataDir,
    pub db_path: PathBuf,
    pub writer: WriterHandle,
    pub readers: Arc<ReaderPool>,
    pub broadcaster: Arc<SessionBroadcaster>,
    _pid_lock: std::fs::File,
}

pub struct LlmLayer {
    pub models: Arc<RwLock<ModelRegistry>>,
    pub providers: Arc<RwLock<Option<Arc<ProviderPool>>>>,
    pub default_model: Arc<std::sync::RwLock<String>>,
    pub ollama_endpoint: String,
}

pub struct NetworkLayer {
    pub ws_addr: SocketAddr,
    pub ws_port: u16,
    pub update_endpoint: String,
}

pub struct SecurityLayer {
    pub authenticator: Arc<dyn Authenticator>,
    pub tls: TlsMaterials,
    pub tls_fingerprint: Option<String>,
    pub pairing: PairingSession,
}

pub struct ToolsLayer {
    pub registry: Arc<Registry>,
    pub skills: Arc<std::sync::RwLock<harness_skills::Catalog>>,
    pub pending_approvals: PendingApprovals,
    pub mcp_supervisor: Arc<McpSupervisor>,
}

struct BootConfig {
    ws_addr: SocketAddr,
    ws_port: u16,
    update_endpoint: String,
    ollama_endpoint: String,
    browser_cdp_endpoint: Option<String>,
    default_model: String,
    tls_extra_dns: Vec<String>,
}

impl Daemon {
    async fn start() -> anyhow::Result<Arc<Self>> {
        let data_dir = DataDir::init(data_dir())?;
        info!(data_dir = %data_dir.root.display(), "initialized data directory");

        let pid_lock = claim_pid_file(&data_dir)?;
        remove_stale_socket(&data_dir);

        let db_path = data_dir.db_path();
        let _db = Database::open(&db_path)?;
        info!(db_path = %db_path.display(), "opened database");
        let writer = Writer::spawn(&db_path)?;
        let readers = Arc::new(ReaderPool::with_defaults(db_path.clone()));
        info!("spawned writer task and reader pool");

        let cfg = load_boot_config(&readers)?;

        let tls = TlsMaterials::load_or_create(
            &data_dir.tls_cert_path(),
            &data_dir.tls_key_path(),
            &cfg.tls_extra_dns,
        )?;
        let tls_fingerprint = Some(tls.fingerprint_hex.clone());
        info!(fingerprint = %tls.fingerprint_hex, "tls materials ready");

        let models = Arc::new(RwLock::new(ModelRegistry::with_builtins()));
        info!(count = models.read().await.len(), "seeded model registry");

        let pool = {
            let reader = readers.get()?;
            providers::build_pool(&reader, &writer)?
        };
        let providers = Arc::new(RwLock::new(Some(Arc::new(pool))));

        run_recovery(&readers, &writer).await;

        let broadcaster = Arc::new(SessionBroadcaster::default());
        let pairing = PairingSession::default();
        let shutdown = Shutdown::new();
        let authenticator: Arc<dyn Authenticator> =
            DeviceAuthenticator::new(writer.clone(), readers.clone(), pairing.clone());

        let skills = Arc::new(std::sync::RwLock::new(discover_skills()));

        let registry = tools::build(RegistryInputs {
            writer: writer.clone(),
            broadcaster: broadcaster.clone(),
            db_path: db_path.clone(),
            default_model: cfg.default_model.clone(),
            browser_cdp_endpoint: cfg.browser_cdp_endpoint,
            skills: skills.clone(),
        });

        let mcp_supervisor = Arc::new(McpSupervisor::new());
        rpc::mcp::boot_all(&readers, &mcp_supervisor, &registry).await;

        let pending_approvals: PendingApprovals = Arc::default();

        Ok(Arc::new(Self {
            storage: StorageLayer {
                data_dir,
                db_path,
                writer,
                readers,
                broadcaster,
                _pid_lock: pid_lock,
            },
            llm: LlmLayer {
                models,
                providers,
                default_model: Arc::new(std::sync::RwLock::new(cfg.default_model)),
                ollama_endpoint: cfg.ollama_endpoint,
            },
            network: NetworkLayer {
                ws_addr: cfg.ws_addr,
                ws_port: cfg.ws_port,
                update_endpoint: cfg.update_endpoint,
            },
            security: SecurityLayer {
                authenticator,
                tls,
                tls_fingerprint,
                pairing,
            },
            tools: ToolsLayer {
                registry,
                skills,
                pending_approvals,
                mcp_supervisor,
            },
            shutdown,
        }))
    }

    fn spawn_backup_scheduler(&self) -> tokio::task::JoinHandle<()> {
        let db_path = self.storage.data_dir.db_path();
        let backup_path = self.storage.data_dir.backup_path();
        let shutdown = self.shutdown.clone();
        tokio::spawn(async move {
            let mut ticker = interval(DAILY);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            ticker.tick().await;
            loop {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    _ = ticker.tick() => {
                        if let Err(e) = run_backup(&db_path, &backup_path).await {
                            warn!(error = %e, "scheduled backup failed");
                        }
                    }
                }
            }
            debug!("backup scheduler exiting");
        })
    }

    fn spawn_update_scheduler(&self) -> tokio::task::JoinHandle<()> {
        let shutdown = self.shutdown.clone();
        let endpoint = self.network.update_endpoint.clone();
        let writer = self.storage.writer.clone();
        tokio::spawn(async move {
            let http = match reqwest::Client::builder()
                .connect_timeout(UPDATE_CONNECT_TIMEOUT)
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    warn!(error = %e, "could not build update http client");
                    return;
                }
            };
            let mut checker = UpdateChecker::new(VERSION, DAILY);
            let mut ticker = interval(UPDATE_TICK);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    _ = ticker.tick() => {
                        let now = tokio::time::Instant::now();
                        if !checker.should_check(now) {
                            continue;
                        }
                        match checker.check(&http, &endpoint).await {
                            Ok(snapshot) => {
                                if snapshot.available {
                                    info!(
                                        current = %snapshot.current,
                                        latest = %snapshot.latest,
                                        "new harness release available",
                                    );
                                    if let Err(e) = cfg_store::set(
                                        &writer,
                                        "update.latest_known".into(),
                                        snapshot.latest.clone(),
                                    )
                                    .await
                                    {
                                        warn!(error = %e, "persist latest version failed");
                                    }
                                }
                            }
                            Err(e) => debug!(error = %e, "update check failed"),
                        }
                        checker.mark_checked(now);
                    }
                }
            }
            debug!("update scheduler exiting");
        })
    }
}

fn load_boot_config(readers: &ReaderPool) -> anyhow::Result<BootConfig> {
    let reader = readers.get()?;
    let ws_port: u16 = cfg_store::get(&reader, "network.ws_port")?
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_WS_PORT);
    let ws_host =
        cfg_store::get(&reader, "network.ws_host")?.unwrap_or_else(|| DEFAULT_WS_HOST.into());
    let ws_addr: SocketAddr = format!("{ws_host}:{ws_port}").parse()?;
    let domain = cfg_store::get(&reader, "network.domain")?;
    let default_model = match cfg_store::get(&reader, "default_model")? {
        Some(m) => m,
        None => providers::suggest_default_model(&reader)?
            .inspect(|m| {
                info!(
                    model = %m,
                    "default_model not set — inferred from registered credentials"
                );
            })
            .unwrap_or_else(|| {
                warn!(
                    "no credentials configured. the TUI will open the auth wizard; \
                     non-interactive callers should run `harness auth login`"
                );
                String::new()
            }),
    };
    let ollama_endpoint = cfg_store::get(&reader, "ollama.endpoint")?
        .unwrap_or_else(|| ollama::DEFAULT_ENDPOINT.to_string());
    let update_endpoint = cfg_store::get(&reader, "network.update_endpoint")?
        .unwrap_or_else(|| DEFAULT_UPDATE_ENDPOINT.into());
    let browser_cdp_endpoint = cfg_store::get(&reader, "browser.cdp_endpoint")?;
    let tls_extra_dns = domain.map(|d| vec![d]).unwrap_or_default();
    Ok(BootConfig {
        ws_addr,
        ws_port,
        update_endpoint,
        ollama_endpoint,
        browser_cdp_endpoint,
        default_model,
        tls_extra_dns,
    })
}

async fn run_recovery(readers: &ReaderPool, writer: &WriterHandle) {
    let reader = match readers.get() {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "recovery: could not open reader");
            return;
        }
    };
    match harness_agent::recover(&reader, writer).await {
        Ok(report) if !report.marked_failed.is_empty() => {
            warn!(
                agents = report.marked_failed.len(),
                "recovered stale agents from previous daemon run"
            );
        }
        Ok(_) => debug!("recovery: no stale agents"),
        Err(e) => error!(error = %e, "recovery pass failed"),
    }
}

fn discover_skills() -> harness_skills::Catalog {
    match harness_skills::DiscoveryConfig::from_env() {
        Ok(cfg) => {
            let catalog = harness_skills::discover(&cfg);
            info!(count = catalog.len(), "discovered agent skills");
            catalog
        }
        Err(e) => {
            warn!(%e, "skill discovery disabled — could not resolve cwd");
            harness_skills::Catalog::new()
        }
    }
}

async fn run(d: Arc<Daemon>) -> anyhow::Result<()> {
    info!(version = VERSION, "harness daemon running");
    let router = rpc::build_router(&d);

    let unix_router = router.clone();
    let unix_shutdown = d.shutdown.clone();
    let unix_path = d.storage.data_dir.socket_path();
    let unix_task = tokio::spawn(async move {
        if let Err(e) = serve_unix(unix_path, unix_router, unix_shutdown).await {
            error!(error = %e, "unix socket server exited");
        }
    });

    let ws_router = router.clone();
    let ws_shutdown = d.shutdown.clone();
    let ws_cfg = WsConfig {
        addr: d.network.ws_addr,
        tls: Some(d.security.tls.clone()),
        authenticator: Some(d.security.authenticator.clone()),
    };
    let ws_task = tokio::spawn(async move {
        if let Err(e) = serve_ws(ws_cfg, ws_router, ws_shutdown).await {
            error!(error = %e, "ws server exited");
        }
    });

    let backup_task = d.spawn_backup_scheduler();
    let update_task = d.spawn_update_scheduler();
    let ollama_task = ollama::spawn(
        d.llm.ollama_endpoint.clone(),
        d.llm.models.clone(),
        d.shutdown.clone(),
    );

    wait_for_shutdown(&d).await?;

    d.shutdown.trigger();
    info!("shutting down");
    d.tools.mcp_supervisor.shutdown().await;
    tokio::time::timeout(SHUTDOWN_GRACE, async {
        unix_task.await.ok();
        ws_task.await.ok();
        backup_task.await.ok();
        update_task.await.ok();
        ollama_task.await.ok();
    })
    .await
    .ok();

    remove_runtime_files(&d.storage.data_dir);

    Ok(())
}

async fn wait_for_shutdown(d: &Arc<Daemon>) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigint = signal(SignalKind::interrupt())?;
        tokio::select! {
            _ = sigterm.recv() => info!("SIGTERM received"),
            _ = sigint.recv() => info!("SIGINT received"),
            () = d.shutdown.cancelled() => info!("internal shutdown"),
        }
    }
    #[cfg(not(unix))]
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => info!("Ctrl-C received"),
            () = d.shutdown.cancelled() => info!("internal shutdown"),
        }
    }
    Ok(())
}

fn claim_pid_file(data_dir: &DataDir) -> anyhow::Result<std::fs::File> {
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom, Write};

    let pid_path = data_dir.pid_path();
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&pid_path)?;
    match file.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            let existing = std::fs::read_to_string(&pid_path).unwrap_or_default();
            anyhow::bail!(
                "harnessd is already running (pid {}, data-dir {})",
                existing.trim(),
                data_dir.root.display(),
            );
        }
        Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
    }
    let mut file = file;
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    write!(file, "{}", std::process::id())?;
    file.sync_all()?;
    Ok(file)
}

fn remove_stale_socket(data_dir: &DataDir) {
    let sock = data_dir.socket_path();
    if sock.exists() {
        std::fs::remove_file(&sock).ok();
    }
}

fn remove_runtime_files(data_dir: &DataDir) {
    std::fs::remove_file(data_dir.pid_path()).ok();
    std::fs::remove_file(data_dir.socket_path()).ok();
}

async fn run_backup(
    db_path: &std::path::Path,
    backup_path: &std::path::Path,
) -> anyhow::Result<()> {
    let db_path = db_path.to_path_buf();
    let dst = backup_path.to_path_buf();
    let dst_for_log = dst.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let src = rusqlite::Connection::open(&db_path)?;
        backup::run(&src, &dst)?;
        Ok(())
    })
    .await??;
    info!(path = %dst_for_log.display(), "daily backup complete");
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};
    let filter = EnvFilter::try_from_env("HARNESS_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,harness=debug"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .init();
}

fn print_usage() {
    eprintln!(
        "harnessd {VERSION}\n\n\
         USAGE:\n    \
             harnessd [FLAGS]\n\n\
         FLAGS:\n    \
             -h, --help       Print this help\n    \
             -V, --version    Print version\n\n\
         ENVIRONMENT:\n    \
             HARNESS_DATA_DIR   Override data directory (default: ~/.harness)\n    \
             HARNESS_LOG        tracing filter (default: info,harness=debug)"
    );
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return Ok(());
            }
            "-V" | "--version" => {
                println!("{VERSION}");
                return Ok(());
            }
            other => {
                eprintln!("unknown argument: {other}");
                print_usage();
                std::process::exit(2);
            }
        }
    }

    init_tracing();

    match Daemon::start().await {
        Ok(d) => run(d).await,
        Err(e) => {
            error!(error = %e, "failed to start daemon");
            Err(e)
        }
    }
}
