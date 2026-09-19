use axum::{
    extract::{DefaultBodyLimit, Json, Multipart, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::{
    net::TcpListener,
    process::{Child, Command},
    sync::Mutex,
    time::sleep,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{info, warn};
use uuid::Uuid;

use crate::commands::{self, settings::AppSettings};
use crate::domain::{config::ProvisioningConfig, node::DiscoveredNode};

const DEFAULT_CONTROL_PORT: u16 = 8090;
const DEFAULT_UI_PORT: u16 = 8080;
const MAX_UPLOAD_BYTES: usize = 128 * 1024 * 1024;
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Parser)]
#[command(
    name = "ruview-control",
    about = "Loopback helper für die RuView-Browseroberfläche"
)]
struct Args {
    /// Bind-Adresse. Nur Loopback-Adressen werden akzeptiert.
    #[arg(long, default_value = "127.0.0.1")]
    bind: IpAddr,

    /// Port des lokalen Helpers.
    #[arg(long, default_value_t = DEFAULT_CONTROL_PORT)]
    port: u16,

    /// Port, auf dem die Browser-UI läuft und mutierende Requests senden darf.
    #[arg(long, default_value_t = DEFAULT_UI_PORT)]
    ui_port: u16,

    /// Persistentes JSON-Launch-Profil des Sensing-Servers.
    #[arg(long)]
    server_spec: Option<PathBuf>,

    /// PID eines bereits laufenden Sensing-Servers, der adoptiert werden soll.
    #[arg(long, hide = true)]
    adopt_pid: Option<u32>,

    /// Sensing-Server-Binary für einen Standalone-Start ohne Profil.
    #[arg(long, env = "RUVIEW_SENSING_SERVER_BIN")]
    server_bin: Option<PathBuf>,

    /// Wiederholbares Argument für den Sensing-Server. Kein Shell-Parsing.
    #[arg(long = "server-arg")]
    server_args: Vec<String>,

    /// Verzeichnis für die lokale Einstellungsdatei.
    #[arg(long)]
    config_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub server_bin: PathBuf,
    pub server_args: Vec<String>,
    pub http_port: u16,
}

struct ControllerState {
    spec: LaunchSpec,
    ui_port: u16,
    control_port: u16,
    pid: Option<u32>,
    child: Option<Child>,
    launch_log_path: Option<PathBuf>,
    busy: bool,
    last_action: Option<String>,
    last_error: Option<String>,
}

type SharedController = Arc<Mutex<ControllerState>>;
type SharedFlashJobs = Arc<Mutex<HashMap<String, FlashJob>>>;

#[derive(Clone)]
struct AppState {
    controller: SharedController,
    flash_jobs: SharedFlashJobs,
    config_path: PathBuf,
    control_port: u16,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }
}

impl From<String> for ApiError {
    fn from(message: String) -> Self {
        Self::bad_request(message)
    }
}

impl From<&str> for ApiError {
    fn from(message: &str) -> Self {
        Self::bad_request(message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    controller: &'static str,
    helper_version: &'static str,
    running: bool,
    pid: Option<u32>,
    http_port: u16,
    control_port: u16,
    ui_url: String,
    busy: bool,
    last_action: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct ActionResponse {
    status: &'static str,
    running: bool,
    pid: Option<u32>,
    message: String,
}

#[derive(Debug, Deserialize)]
struct AdoptRequest {
    server_bin: PathBuf,
    server_args: Vec<String>,
    http_port: u16,
    ui_port: u16,
    pid: u32,
}

#[derive(Debug, Serialize)]
struct HelperInfo {
    helper_version: &'static str,
    loopback_only: bool,
    control_port: u16,
    capabilities: [&'static str; 8],
}

pub async fn run() -> Result<(), String> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into()),
        )
        .init();

    let args = Args::parse();
    if !args.bind.is_loopback() {
        return Err("Der Control-Helper darf nur an Loopback gebunden werden".to_string());
    }
    if args.port == 0 || args.ui_port == 0 {
        return Err("Helper- und UI-Port dürfen nicht 0 sein".to_string());
    }

    let spec = build_launch_spec(&args)?;
    let config_path = args
        .config_dir
        .map(|path| path.join("settings.json"))
        .unwrap_or_else(commands::settings::default_settings_path);
    let controller = Arc::new(Mutex::new(ControllerState {
        spec,
        ui_port: args.ui_port,
        control_port: args.port,
        pid: args.adopt_pid,
        child: None,
        launch_log_path: None,
        busy: false,
        last_action: None,
        last_error: None,
    }));
    let state = AppState {
        controller,
        flash_jobs: Arc::new(Mutex::new(HashMap::new())),
        config_path,
        control_port: args.port,
    };

    let app = router(state);
    let address = SocketAddr::from((args.bind, args.port));
    let listener = TcpListener::bind(address)
        .await
        .map_err(|error| format!("Control-Helper konnte {address} nicht öffnen: {error}"))?;
    info!(%address, "RuView browser control helper listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|error| format!("Control-Helper wurde beendet: {error}"))
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(control_page))
        .route("/status", get(status))
        .route("/start", post(start))
        .route("/stop", post(stop))
        .route("/restart", post(restart))
        .route("/adopt", post(adopt))
        .route("/api/v1/helper/info", get(helper_info))
        .route("/api/v1/nodes/discover", get(discover_nodes))
        .route("/api/v1/serial/ports", get(list_serial_ports))
        .route("/api/v1/serial/wifi", post(configure_wifi))
        .route("/api/v1/provision/validate", post(validate_provisioning))
        .route("/api/v1/provision/mesh", post(generate_mesh_configs))
        .route("/api/v1/provision/node", post(provision_node))
        .route("/api/v1/provision/read", post(read_nvs))
        .route("/api/v1/provision/erase", post(erase_nvs))
        .route("/api/v1/firmware/espflash", get(check_espflash))
        .route("/api/v1/firmware/chips", get(supported_chips))
        .route("/api/v1/firmware/flash", post(flash_firmware))
        .route("/api/v1/firmware/flash/status", get(flash_status))
        .route("/api/v1/firmware/verify", post(verify_firmware))
        .route("/api/v1/ota/check", get(check_ota))
        .route("/api/v1/ota/update", post(ota_update))
        .route("/api/v1/ota/batch", post(batch_ota_update))
        .route("/api/v1/wasm/list", get(wasm_list))
        .route("/api/v1/wasm/upload", post(wasm_upload))
        .route("/api/v1/wasm/control", post(wasm_control))
        .route("/api/v1/wasm/info", get(wasm_info))
        .route("/api/v1/wasm/stats", get(wasm_stats))
        .route("/api/v1/wasm/support", get(wasm_support))
        .route("/api/v1/settings", get(get_settings).put(save_settings))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn build_launch_spec(args: &Args) -> Result<LaunchSpec, String> {
    if let Some(path) = args.server_spec.as_deref() {
        let bytes = fs::read(path).map_err(|error| {
            format!("Server-Launch-Profil konnte nicht gelesen werden: {error}")
        })?;
        let spec: LaunchSpec = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Server-Launch-Profil ist ungültig: {error}"))?;
        validate_launch_spec(spec)
    } else {
        let server_bin = args
            .server_bin
            .clone()
            .or_else(find_sensing_server)
            .ok_or_else(|| {
                "Keine sensing-server-Binary gefunden; setze --server-bin oder --server-spec"
                    .to_string()
            })?;
        let server_args = if args.server_args.is_empty() {
            vec!["--http-port".to_string(), args.ui_port.to_string()]
        } else {
            args.server_args.clone()
        };
        validate_launch_spec(LaunchSpec {
            server_bin,
            server_args,
            http_port: args.ui_port,
        })
    }
}

fn validate_launch_spec(spec: LaunchSpec) -> Result<LaunchSpec, String> {
    if !spec.server_bin.is_file() {
        return Err(format!(
            "Sensing-Server-Binary nicht gefunden: {}",
            spec.server_bin.display()
        ));
    }
    if spec.http_port == 0 {
        return Err("HTTP-Port des Sensing-Servers darf nicht 0 sein".to_string());
    }
    Ok(spec)
}

fn find_sensing_server() -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    if let Some(parent) = current_exe.parent() {
        let sibling = parent.join("sensing-server");
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join("sensing-server"))
        .find(|candidate| candidate.is_file())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM-Handler konnte nicht installiert werden");
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result.expect("CTRL+C-Handler konnte nicht installiert werden");
            }
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("CTRL+C-Handler konnte nicht installiert werden");
    }
}

async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    let mut controller = state.controller.lock().await;
    refresh_process(&mut controller);
    Json(status_response(&controller))
}

async fn helper_info(State(state): State<AppState>) -> Json<HelperInfo> {
    Json(HelperInfo {
        helper_version: env!("CARGO_PKG_VERSION"),
        loopback_only: true,
        control_port: state.control_port,
        capabilities: [
            "server",
            "node_discovery",
            "serial",
            "firmware_flash",
            "ota",
            "wasm",
            "provisioning",
            "settings",
        ],
    })
}

async fn adopt(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AdoptRequest>,
) -> Result<Json<ActionResponse>, ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    if request.pid == 0 || request.ui_port == 0 {
        return Err(ApiError::bad_request(
            "PID und UI-Port für die Helper-Übernahme müssen gültig sein",
        ));
    }
    #[cfg(unix)]
    if !process_exists(request.pid) {
        return Err(ApiError::bad_request(format!(
            "Sensing-Server-PID {} ist nicht aktiv",
            request.pid
        )));
    }

    let spec = validate_launch_spec(LaunchSpec {
        server_bin: request.server_bin,
        server_args: request.server_args,
        http_port: request.http_port,
    })?;
    let mut controller = begin_action(&state.controller, "adopt").await?;
    refresh_process(&mut controller);
    if let Some(existing_pid) = controller.pid {
        if existing_pid != request.pid {
            controller.busy = false;
            return Err(ApiError::conflict(format!(
                "Der Helper überwacht bereits Sensing-Server-PID {existing_pid}"
            )));
        }
    }
    controller.spec = spec;
    controller.ui_port = request.ui_port;
    controller.pid = Some(request.pid);
    controller.child = None;
    remove_launch_log(&mut controller);
    controller.last_error = None;
    let response = action_response(
        &controller,
        "adopted",
        "Der laufende Sensing-Server wurde vom Helper übernommen.",
    );
    controller.busy = false;
    Ok(Json(response))
}

async fn start(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ActionResponse>, ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    let mut controller = begin_action(&state.controller, "start").await?;
    refresh_process(&mut controller);
    if controller.pid.is_some() {
        let response = action_response(
            &controller,
            "already_running",
            "Sensing-Server läuft bereits.",
        );
        controller.busy = false;
        return Ok(Json(response));
    }
    match spawn_server(&mut controller) {
        Ok(()) => {
            let response =
                action_response(&controller, "started", "Sensing-Server wurde gestartet.");
            controller.busy = false;
            Ok(Json(response))
        }
        Err(error) => Err(action_failure(&mut controller, error)),
    }
}

async fn stop(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ActionResponse>, ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    let pid = {
        let mut controller = begin_action(&state.controller, "stop").await?;
        refresh_process(&mut controller);
        let pid = controller.pid;
        if pid.is_none() {
            controller.busy = false;
            return Ok(Json(action_response(
                &controller,
                "stopped",
                "Sensing-Server läuft nicht.",
            )));
        }
        pid.expect("pid checked above")
    };
    if let Err(error) = terminate_process(pid) {
        let mut controller = state.controller.lock().await;
        return Err(action_failure(&mut controller, error));
    }
    if !wait_for_stop(&state.controller, pid).await {
        let mut controller = state.controller.lock().await;
        controller.last_error = Some(format!(
            "Sensing-Server {pid} reagiert nicht auf das Stoppsignal"
        ));
        controller.busy = false;
        return Err(ApiError {
            status: StatusCode::GATEWAY_TIMEOUT,
            message: controller.last_error.clone().unwrap_or_default(),
        });
    }
    let mut controller = state.controller.lock().await;
    controller.pid = None;
    controller.child = None;
    remove_launch_log(&mut controller);
    controller.last_error = None;
    controller.busy = false;
    Ok(Json(action_response(
        &controller,
        "stopped",
        "Sensing-Server wurde beendet.",
    )))
}

async fn restart(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ActionResponse>, ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    let pid = {
        let mut controller = begin_action(&state.controller, "restart").await?;
        refresh_process(&mut controller);
        controller.pid
    };
    if let Some(pid) = pid {
        if let Err(error) = terminate_process(pid) {
            let mut controller = state.controller.lock().await;
            return Err(action_failure(&mut controller, error));
        }
        if !wait_for_stop(&state.controller, pid).await {
            let mut controller = state.controller.lock().await;
            controller.last_error = Some(format!(
                "Sensing-Server {pid} reagiert nicht auf den Neustart"
            ));
            controller.busy = false;
            return Err(ApiError {
                status: StatusCode::GATEWAY_TIMEOUT,
                message: controller.last_error.clone().unwrap_or_default(),
            });
        }
    }
    let mut controller = state.controller.lock().await;
    controller.pid = None;
    controller.child = None;
    match spawn_server(&mut controller) {
        Ok(()) => {
            let response = action_response(
                &controller,
                "restarted",
                "Sensing-Server wurde neu gestartet.",
            );
            controller.busy = false;
            Ok(Json(response))
        }
        Err(error) => Err(action_failure(&mut controller, error)),
    }
}

async fn ensure_mutation_origin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(());
    };
    let controller = state.controller.lock().await;
    let allowed = [
        format!("http://localhost:{}", controller.ui_port),
        format!("http://127.0.0.1:{}", controller.ui_port),
        format!("http://[::1]:{}", controller.ui_port),
        format!("http://localhost:{}", state.control_port),
        format!("http://127.0.0.1:{}", state.control_port),
        format!("http://[::1]:{}", state.control_port),
    ];
    if allowed.iter().any(|candidate| candidate == origin) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "Browser-Origin für den lokalen Helper ist nicht erlaubt",
        ))
    }
}

async fn begin_action<'a>(
    controller: &'a SharedController,
    action: &str,
) -> Result<tokio::sync::MutexGuard<'a, ControllerState>, ApiError> {
    let mut controller = controller.lock().await;
    if controller.busy {
        return Err(ApiError::conflict(
            "Eine andere Helper-Aktion läuft bereits",
        ));
    }
    controller.busy = true;
    controller.last_action = Some(action.to_string());
    Ok(controller)
}

fn action_failure(controller: &mut ControllerState, error: String) -> ApiError {
    controller.last_error = Some(error.clone());
    controller.busy = false;
    ApiError::internal(error)
}

fn refresh_process(controller: &mut ControllerState) {
    if let Some(child) = controller.child.as_mut() {
        match child.try_wait() {
            Ok(Some(exit)) => {
                let diagnostic = controller
                    .launch_log_path
                    .take()
                    .and_then(|path| read_launch_diagnostic(&path));
                if !exit.success() {
                    controller.last_error = Some(match diagnostic {
                        Some(diagnostic) => {
                            format!("Sensing-Server beendet mit {exit}: {diagnostic}")
                        }
                        None => format!("Sensing-Server beendet mit {exit}"),
                    });
                }
                controller.child = None;
                controller.pid = None;
            }
            Ok(None) => {}
            Err(error) => {
                remove_launch_log(controller);
                controller.last_error =
                    Some(format!("Serverstatus konnte nicht gelesen werden: {error}"));
                controller.child = None;
                controller.pid = None;
            }
        }
    } else if let Some(pid) = controller.pid {
        if !process_exists(pid) {
            controller.pid = None;
        }
    }
}

fn spawn_server(controller: &mut ControllerState) -> Result<(), String> {
    let (log_path, log_file) = create_launch_log()?;
    let mut command = Command::new(&controller.spec.server_bin);
    command
        .args(&controller.spec.server_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log_file));
    let child = command
        .spawn()
        .map_err(|error| {
            let _ = fs::remove_file(&log_path);
            format!("Sensing-Server konnte nicht gestartet werden: {error}")
        })?;
    let pid = child
        .id()
        .ok_or_else(|| {
            let _ = fs::remove_file(&log_path);
            "Sensing-Server-PID konnte nicht ermittelt werden".to_string()
        })?;
    controller.child = Some(child);
    controller.pid = Some(pid);
    controller.launch_log_path = Some(log_path);
    controller.last_error = None;
    Ok(())
}

fn create_launch_log() -> Result<(PathBuf, fs::File), String> {
    let unique_suffix = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let path = std::env::temp_dir().join(format!(
        "ruview-sensing-server-{}-{unique_suffix}.log",
        std::process::id()
    ));
    let file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(|error| format!("Serverdiagnose konnte nicht angelegt werden: {error}"))?;
    Ok((path, file))
}

fn remove_launch_log(controller: &mut ControllerState) {
    if let Some(path) = controller.launch_log_path.take() {
        let _ = fs::remove_file(path);
    }
}

fn read_launch_diagnostic(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok();
    let _ = fs::remove_file(path);
    let text = String::from_utf8_lossy(bytes.as_deref()?).to_string();
    let mut lines = text.lines().rev().take(12).collect::<Vec<_>>();
    lines.reverse();
    let detail = lines.join("\n");
    if detail.trim().is_empty() {
        return None;
    }
    let detail = detail.trim();
    let truncated = if detail.chars().count() > 6000 {
        let suffix = detail.chars().rev().take(6000).collect::<String>();
        suffix.chars().rev().collect::<String>()
    } else {
        detail.to_string()
    };
    Some(truncated)
}

async fn wait_for_stop(controller: &SharedController, pid: u32) -> bool {
    let deadline = tokio::time::Instant::now() + STOP_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        {
            let mut controller = controller.lock().await;
            refresh_process(&mut controller);
            if controller.pid != Some(pid) {
                return true;
            }
        }
        sleep(Duration::from_millis(100)).await;
    }
    if process_exists(pid) {
        if let Err(error) = force_kill_process(pid) {
            warn!(pid, %error, "Sensing-Server konnte nicht erzwungen beendet werden");
            return false;
        }
        // An adopted process cannot be reaped by the helper. Once the OS has
        // accepted SIGKILL, waiting for kill(pid, 0) would misclassify a
        // short-lived zombie as a still-running server.
        return true;
    }
    true
}

fn status_response(controller: &ControllerState) -> StatusResponse {
    StatusResponse {
        controller: "ready",
        helper_version: env!("CARGO_PKG_VERSION"),
        running: controller.pid.is_some(),
        pid: controller.pid,
        http_port: controller.spec.http_port,
        control_port: controller.control_port,
        ui_url: format!(
            "http://localhost:{}/ui/index.html#sensing",
            controller.spec.http_port
        ),
        busy: controller.busy,
        last_action: controller.last_action.clone(),
        error: controller.last_error.clone(),
    }
}

fn action_response(
    controller: &ControllerState,
    status: &'static str,
    message: &str,
) -> ActionResponse {
    ActionResponse {
        status,
        running: controller.pid.is_some(),
        pid: controller.pid,
        message: message.to_string(),
    }
}

async fn control_page() -> Html<&'static str> {
    Html(
        r##"<!doctype html><html lang="de"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>RuView Control-Helper</title><style>body{font:16px system-ui;max-width:42rem;margin:3rem auto;padding:0 1rem;background:#10131a;color:#eef2f7}main{padding:1.5rem;border:1px solid #303846;border-radius:1rem;background:#181d26}.actions{display:flex;flex-wrap:wrap;gap:.75rem;margin:1.5rem 0}button{padding:.7rem 1rem;border:1px solid #56647a;border-radius:.6rem;background:#263246;color:inherit;cursor:pointer}button:disabled{opacity:.45}#state{font-weight:700}</style></head><body><main><h1>RuView Control-Helper</h1><p id="state">Status wird geladen …</p><div class="actions"><button data-action="start">Server starten</button><button data-action="restart">Neu starten</button><button data-action="stop">Server stoppen</button><button data-action="status">Status prüfen</button></div><p id="message" role="status"></p><p><a href="/api/v1/helper/info">Helper-Funktionen als JSON</a></p></main><script>const state=document.querySelector('#state'),message=document.querySelector('#message');async function refresh(){const response=await fetch('/status',{cache:'no-store'});const p=await response.json();state.textContent=p.running?'Server läuft'+(p.pid?' · PID '+p.pid:''):'Server ist gestoppt';message.textContent=p.error||''}async function action(name){if(name==='status')return refresh();try{const response=await fetch('/'+name,{method:'POST'});const p=await response.json();if(!response.ok)throw Error(p.error||'Aktion fehlgeschlagen');message.textContent=p.message;await refresh()}catch(error){message.textContent=error.message}}document.addEventListener('click',event=>{const button=event.target.closest('[data-action]');if(button)void action(button.dataset.action)});void refresh();setInterval(()=>void refresh(),3000)</script></body></html>"##,
    )
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn process_exists(_pid: u32) -> bool {
    false
}

#[cfg(unix)]
fn terminate_process(pid: u32) -> Result<(), String> {
    let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    if result == 0 || !process_exists(pid) {
        Ok(())
    } else {
        Err(format!(
            "Sensing-Server {pid} konnte nicht beendet werden: {}",
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(not(unix))]
fn terminate_process(_pid: u32) -> Result<(), String> {
    Err(
        "Serversteuerung per Prozesssignal ist auf diesem Betriebssystem nicht implementiert"
            .to_string(),
    )
}

#[cfg(unix)]
fn force_kill_process(pid: u32) -> Result<(), String> {
    let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
    if result == 0 || !process_exists(pid) {
        Ok(())
    } else {
        Err(format!(
            "Sensing-Server {pid} konnte nicht erzwungen beendet werden: {}",
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(not(unix))]
fn force_kill_process(_pid: u32) -> Result<(), String> {
    Err(
        "Serversteuerung per Prozesssignal ist auf diesem Betriebssystem nicht implementiert"
            .to_string(),
    )
}

#[derive(Debug)]
struct UploadedFile {
    filename: String,
    bytes: Vec<u8>,
}

async fn read_multipart(
    mut multipart: Multipart,
) -> Result<(HashMap<String, String>, UploadedFile), ApiError> {
    let mut fields = HashMap::new();
    let mut upload = None;
    while let Some(field) = multipart.next_field().await.map_err(|error| {
        ApiError::bad_request(format!(
            "Multipart-Feld konnte nicht gelesen werden: {error}"
        ))
    })? {
        let name = field.name().unwrap_or_default().to_string();
        if matches!(name.as_str(), "file" | "firmware" | "wasm") {
            let filename = field
                .file_name()
                .map(sanitize_filename)
                .unwrap_or_else(|| "upload.bin".to_string());
            let bytes = field.bytes().await.map_err(|error| {
                ApiError::bad_request(format!("Upload konnte nicht gelesen werden: {error}"))
            })?;
            if bytes.is_empty() {
                return Err(ApiError::bad_request("Upload-Datei ist leer"));
            }
            upload = Some(UploadedFile {
                filename,
                bytes: bytes.to_vec(),
            });
        } else {
            let value = field.text().await.map_err(|error| {
                ApiError::bad_request(format!(
                    "Multipart-Feld konnte nicht gelesen werden: {error}"
                ))
            })?;
            fields.insert(name, value);
        }
    }
    let upload = upload.ok_or_else(|| ApiError::bad_request("Keine Upload-Datei übergeben"))?;
    Ok((fields, upload))
}

fn sanitize_filename(filename: &str) -> String {
    let basename = Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("upload.bin");
    let sanitized = basename
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "upload.bin".to_string()
    } else {
        sanitized
    }
}

async fn store_upload(upload: &UploadedFile, suffix: &str) -> Result<PathBuf, ApiError> {
    if upload.bytes.len() > MAX_UPLOAD_BYTES {
        return Err(ApiError::bad_request(format!(
            "Upload ist zu groß (maximal {} MiB)",
            MAX_UPLOAD_BYTES / 1024 / 1024
        )));
    }
    let path = std::env::temp_dir().join(format!(
        "ruview-control-{}-{}.{}",
        std::process::id(),
        Uuid::new_v4(),
        suffix.trim_start_matches('.')
    ));
    tokio::fs::write(&path, &upload.bytes)
        .await
        .map_err(|error| {
            ApiError::internal(format!(
                "Temporäre Upload-Datei konnte nicht geschrieben werden: {error}"
            ))
        })?;
    Ok(path)
}

fn ensure_upload_extension(upload: &UploadedFile, expected: &str) -> Result<(), ApiError> {
    let extension = Path::new(&upload.filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if !extension.is_empty() && !extension.eq_ignore_ascii_case(expected) {
        return Err(ApiError::bad_request(format!(
            "Upload-Datei muss die Endung .{expected} haben"
        )));
    }
    Ok(())
}

async fn remove_upload(path: &Path) {
    if let Err(error) = tokio::fs::remove_file(path).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            warn!(path = %path.display(), %error, "Temporäre Upload-Datei konnte nicht gelöscht werden");
        }
    }
}

fn required_field<'a>(
    fields: &'a HashMap<String, String>,
    name: &str,
) -> Result<&'a str, ApiError> {
    fields
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::bad_request(format!("Multipart-Feld '{name}' fehlt")))
}

fn optional_u32(fields: &HashMap<String, String>, name: &str) -> Result<Option<u32>, ApiError> {
    fields
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            value
                .parse::<u32>()
                .map_err(|_| ApiError::bad_request(format!("Feld '{name}' ist keine gültige Zahl")))
        })
        .transpose()
}

fn optional_bool(fields: &HashMap<String, String>, name: &str) -> Result<Option<bool>, ApiError> {
    fields
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            value
                .parse::<bool>()
                .map_err(|_| ApiError::bad_request(format!("Feld '{name}' ist kein true/false")))
        })
        .transpose()
}

fn validate_node_ip(raw: &str) -> Result<String, ApiError> {
    let ip = raw
        .parse::<IpAddr>()
        .map_err(|_| ApiError::bad_request("Node-IP muss eine numerische IP-Adresse sein"))?;
    let allowed = match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local(),
    };
    if !allowed {
        return Err(ApiError::bad_request(
            "Node-IP muss im lokalen/private Netz liegen",
        ));
    }
    Ok(ip.to_string())
}

fn validate_module_id(module_id: &str) -> Result<(), ApiError> {
    if module_id.is_empty()
        || module_id.len() > 64
        || !module_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(ApiError::bad_request("WASM-Modul-ID ist ungültig"));
    }
    Ok(())
}

async fn discover_nodes(
    State(_state): State<AppState>,
    Query(query): Query<DiscoverQuery>,
) -> Result<Json<Vec<DiscoveredNode>>, ApiError> {
    let timeout_ms = query.timeout_ms.unwrap_or(3_000).clamp(100, 10_000);
    commands::discovery::discover_nodes(Some(timeout_ms))
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

#[derive(Debug, Deserialize)]
struct DiscoverQuery {
    timeout_ms: Option<u64>,
}

async fn list_serial_ports(
    State(_state): State<AppState>,
) -> Result<Json<Vec<commands::discovery::SerialPortInfo>>, ApiError> {
    commands::discovery::list_serial_ports()
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

#[derive(Debug, Deserialize)]
struct WifiRequest {
    port: String,
    ssid: String,
    password: String,
}

async fn configure_wifi(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<WifiRequest>,
) -> Result<Json<MessageResponse>, ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    let message =
        commands::discovery::configure_esp32_wifi(request.port, request.ssid, request.password)
            .await
            .map_err(ApiError::bad_request)?;
    Ok(Json(MessageResponse { message }))
}

#[derive(Debug, Serialize)]
struct MessageResponse {
    message: String,
}

#[derive(Debug, Deserialize)]
struct ProvisionRequest {
    port: String,
    config: ProvisioningConfig,
}

#[derive(Debug, Deserialize)]
struct MeshRequest {
    base_config: ProvisioningConfig,
    node_count: u8,
}

async fn validate_provisioning(
    State(_state): State<AppState>,
    Json(config): Json<ProvisioningConfig>,
) -> Result<Json<commands::provision::ValidationResult>, ApiError> {
    commands::provision::validate_config(config)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn generate_mesh_configs(
    State(_state): State<AppState>,
    Json(request): Json<MeshRequest>,
) -> Result<Json<Vec<commands::provision::MeshNodeConfig>>, ApiError> {
    commands::provision::generate_mesh_configs(request.base_config, request.node_count)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn provision_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ProvisionRequest>,
) -> Result<Json<commands::provision::ProvisionResult>, ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    commands::provision::provision_node(request.port, request.config)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

#[derive(Debug, Deserialize)]
struct PortRequest {
    port: String,
}

async fn read_nvs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PortRequest>,
) -> Result<Json<ProvisioningConfig>, ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    commands::provision::read_nvs(request.port)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn erase_nvs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PortRequest>,
) -> Result<Json<commands::provision::ProvisionResult>, ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    commands::provision::erase_nvs(request.port)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

#[derive(Debug, Clone, Serialize)]
struct FlashJob {
    session_id: String,
    port: String,
    status: String,
    phase: String,
    progress_pct: f32,
    bytes_total: u64,
    result: Option<commands::flash::FlashResult>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct FlashSessionResponse {
    session_id: String,
    status: &'static str,
}

async fn check_espflash(
    State(_state): State<AppState>,
) -> Result<Json<commands::flash::EspflashInfo>, ApiError> {
    commands::flash::check_espflash()
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn supported_chips(State(_state): State<AppState>) -> Json<Vec<commands::flash::ChipInfo>> {
    Json(commands::flash::supported_chips())
}

async fn flash_firmware(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<(StatusCode, Json<FlashSessionResponse>), ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    let (fields, upload) = read_multipart(multipart).await?;
    ensure_upload_extension(&upload, "bin")?;
    let port = required_field(&fields, "port")?.to_string();
    commands::discovery::validate_serial_port_path(&port).map_err(ApiError::bad_request)?;
    let chip = fields
        .get("chip")
        .cloned()
        .filter(|value| !value.is_empty());
    let baud = optional_u32(&fields, "baud")?;
    let running = {
        let jobs = state.flash_jobs.lock().await;
        jobs.values().any(|job| job.status == "running")
    };
    if running {
        return Err(ApiError::conflict("Ein Firmware-Flash läuft bereits"));
    }
    let path = store_upload(&upload, "bin").await?;
    let session_id = Uuid::new_v4().to_string();
    let bytes_total = upload.bytes.len() as u64;
    {
        let mut jobs = state.flash_jobs.lock().await;
        jobs.insert(
            session_id.clone(),
            FlashJob {
                session_id: session_id.clone(),
                port: port.clone(),
                status: "running".to_string(),
                phase: "connecting".to_string(),
                progress_pct: 0.0,
                bytes_total,
                result: None,
                error: None,
            },
        );
    }

    let jobs = state.flash_jobs.clone();
    let session_for_task = session_id.clone();
    tokio::spawn(async move {
        let result =
            commands::flash::flash_firmware(port, path.to_string_lossy().into_owned(), chip, baud)
                .await;
        remove_upload(&path).await;
        let mut jobs = jobs.lock().await;
        if let Some(job) = jobs.get_mut(&session_for_task) {
            match result {
                Ok(result) => {
                    job.status = "completed".to_string();
                    job.phase = "completed".to_string();
                    job.progress_pct = 100.0;
                    job.result = Some(result);
                }
                Err(error) => {
                    job.status = "failed".to_string();
                    job.phase = "failed".to_string();
                    job.error = Some(error);
                }
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(FlashSessionResponse {
            session_id,
            status: "started",
        }),
    ))
}

#[derive(Debug, Deserialize)]
struct FlashStatusQuery {
    session_id: String,
}

async fn flash_status(
    State(state): State<AppState>,
    Query(query): Query<FlashStatusQuery>,
) -> Result<Json<FlashJob>, ApiError> {
    let jobs = state.flash_jobs.lock().await;
    jobs.get(&query.session_id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::bad_request("Flash-Session nicht gefunden"))
}

async fn verify_firmware(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<commands::flash::VerifyResult>, ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    let (fields, upload) = read_multipart(multipart).await?;
    ensure_upload_extension(&upload, "bin")?;
    let port = required_field(&fields, "port")?.to_string();
    commands::discovery::validate_serial_port_path(&port).map_err(ApiError::bad_request)?;
    let chip = fields.get("chip").cloned();
    let path = store_upload(&upload, "bin").await?;
    let result = commands::flash::verify_firmware(port, path.to_string_lossy().into_owned(), chip)
        .await
        .map(Json)
        .map_err(ApiError::bad_request);
    remove_upload(&path).await;
    result
}

#[derive(Debug, Deserialize)]
struct OtaQuery {
    node_ip: String,
}

async fn check_ota(
    State(_state): State<AppState>,
    Query(query): Query<OtaQuery>,
) -> Result<Json<commands::ota::OtaEndpointInfo>, ApiError> {
    let node_ip = validate_node_ip(&query.node_ip)?;
    commands::ota::check_ota_endpoint(node_ip)
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

async fn ota_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<commands::ota::OtaResult>, ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    let (fields, upload) = read_multipart(multipart).await?;
    ensure_upload_extension(&upload, "bin")?;
    let node_ip = validate_node_ip(required_field(&fields, "node_ip")?)?;
    let psk = fields.get("psk").cloned().filter(|value| !value.is_empty());
    let path = store_upload(&upload, "bin").await?;
    let result = commands::ota::ota_update(node_ip, path.to_string_lossy().into_owned(), psk)
        .await
        .map(Json)
        .map_err(ApiError::bad_request);
    remove_upload(&path).await;
    result
}

async fn batch_ota_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<commands::ota::BatchOtaResult>, ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    let (fields, upload) = read_multipart(multipart).await?;
    ensure_upload_extension(&upload, "bin")?;
    let raw_ips = required_field(&fields, "node_ips")?;
    let ips: Vec<String> = serde_json::from_str(raw_ips)
        .or_else(|_| {
            Ok::<_, serde_json::Error>(
                raw_ips
                    .split(',')
                    .map(str::trim)
                    .map(str::to_string)
                    .collect(),
            )
        })
        .map_err(|error| ApiError::bad_request(format!("node_ips ist ungültig: {error}")))?;
    let mut validated_ips = Vec::with_capacity(ips.len());
    for ip in ips {
        validated_ips.push(validate_node_ip(&ip)?);
    }
    let psk = fields.get("psk").cloned().filter(|value| !value.is_empty());
    let strategy = fields.get("strategy").cloned();
    let max_concurrent = optional_u32(&fields, "max_concurrent")?.map(|value| value as usize);
    let path = store_upload(&upload, "bin").await?;
    let result = commands::ota::batch_ota_update(
        validated_ips,
        path.to_string_lossy().into_owned(),
        psk,
        strategy,
        max_concurrent,
    )
    .await
    .map(Json)
    .map_err(ApiError::bad_request);
    remove_upload(&path).await;
    result
}

async fn wasm_list(
    State(_state): State<AppState>,
    Query(query): Query<OtaQuery>,
) -> Result<Json<Vec<commands::wasm::WasmModuleInfo>>, ApiError> {
    let node_ip = validate_node_ip(&query.node_ip)?;
    commands::wasm::wasm_list(node_ip)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn wasm_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<commands::wasm::WasmUploadResult>, ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    let (fields, upload) = read_multipart(multipart).await?;
    ensure_upload_extension(&upload, "wasm")?;
    if upload.bytes.len() < 4 || &upload.bytes[..4] != b"\0asm" {
        return Err(ApiError::bad_request("Upload ist keine gültige WASM-Datei"));
    }
    let node_ip = validate_node_ip(required_field(&fields, "node_ip")?)?;
    let module_name = fields.get("module_name").cloned();
    if let Some(name) = module_name.as_deref() {
        validate_module_id(name)?;
    }
    let auto_start = optional_bool(&fields, "auto_start")?;
    let path = store_upload(&upload, "wasm").await?;
    let result = commands::wasm::wasm_upload(
        node_ip,
        path.to_string_lossy().into_owned(),
        module_name,
        auto_start,
    )
    .await
    .map(Json)
    .map_err(ApiError::bad_request);
    remove_upload(&path).await;
    result
}

#[derive(Debug, Deserialize)]
struct WasmControlRequest {
    node_ip: String,
    module_id: String,
    action: String,
}

async fn wasm_control(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<WasmControlRequest>,
) -> Result<Json<commands::wasm::WasmControlResult>, ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    let node_ip = validate_node_ip(&request.node_ip)?;
    validate_module_id(&request.module_id)?;
    commands::wasm::wasm_control(node_ip, request.module_id, request.action)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

#[derive(Debug, Deserialize)]
struct WasmInfoQuery {
    node_ip: String,
    module_id: String,
}

async fn wasm_info(
    State(_state): State<AppState>,
    Query(query): Query<WasmInfoQuery>,
) -> Result<Json<commands::wasm::WasmModuleDetail>, ApiError> {
    let node_ip = validate_node_ip(&query.node_ip)?;
    validate_module_id(&query.module_id)?;
    commands::wasm::wasm_info(node_ip, query.module_id)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn wasm_stats(
    State(_state): State<AppState>,
    Query(query): Query<OtaQuery>,
) -> Result<Json<commands::wasm::WasmRuntimeStats>, ApiError> {
    let node_ip = validate_node_ip(&query.node_ip)?;
    commands::wasm::wasm_stats(node_ip)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn wasm_support(
    State(_state): State<AppState>,
    Query(query): Query<OtaQuery>,
) -> Result<Json<commands::wasm::WasmSupportInfo>, ApiError> {
    let node_ip = validate_node_ip(&query.node_ip)?;
    commands::wasm::check_wasm_support(node_ip)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

#[derive(Debug, Serialize)]
struct SettingsResponse {
    settings: AppSettings,
    ota_psk_set: bool,
}

async fn get_settings(State(state): State<AppState>) -> Result<Json<SettingsResponse>, ApiError> {
    let settings = tokio::task::spawn_blocking({
        let path = state.config_path.clone();
        move || commands::settings::load(&path)
    })
    .await
    .map_err(|error| ApiError::internal(error.to_string()))?
    .map_err(ApiError::internal)?
    .unwrap_or_default();
    let ota_psk_set = !settings.ota_psk.is_empty();
    let mut response_settings = settings;
    response_settings.ota_psk.clear();
    Ok(Json(SettingsResponse {
        settings: response_settings,
        ota_psk_set,
    }))
}

async fn save_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut settings): Json<AppSettings>,
) -> Result<StatusCode, ApiError> {
    ensure_mutation_origin(&state, &headers).await?;
    if settings.server_http_port == 0
        || settings.server_ws_port == 0
        || settings.server_udp_port == 0
    {
        return Err(ApiError::bad_request("Server-Ports dürfen nicht 0 sein"));
    }
    let path = state.config_path.clone();
    let previous = tokio::task::spawn_blocking({
        let path = path.clone();
        move || commands::settings::load(&path)
    })
    .await
    .map_err(|error| ApiError::internal(error.to_string()))?
    .map_err(ApiError::internal)?;
    if settings.ota_psk.is_empty() {
        settings.ota_psk = previous
            .map(|previous| previous.ota_psk)
            .unwrap_or_default();
    }
    tokio::task::spawn_blocking(move || commands::settings::save(&path, &settings))
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_failure_releases_busy_state() {
        let mut controller = ControllerState {
            spec: LaunchSpec {
                server_bin: PathBuf::from("sensing-server"),
                server_args: Vec::new(),
                http_port: DEFAULT_UI_PORT,
            },
            ui_port: DEFAULT_UI_PORT,
            control_port: DEFAULT_CONTROL_PORT,
            pid: None,
            child: None,
            launch_log_path: None,
            busy: true,
            last_action: Some("stop".to_string()),
            last_error: None,
        };

        let error = action_failure(&mut controller, "stop failed".to_string());

        assert!(!controller.busy);
        assert_eq!(controller.last_error.as_deref(), Some("stop failed"));
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.message, "stop failed");
    }
}
