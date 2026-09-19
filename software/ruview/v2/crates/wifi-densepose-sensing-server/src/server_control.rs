//! Local browser control plane for the sensing-server process.
//!
//! A browser page cannot start an operating-system process by itself.  The
//! sensing server therefore starts this loopback-only helper once and the UI
//! talks to it for start/stop/restart actions.  The helper owns no sensing
//! state; it only supervises the exact server executable and arguments that
//! launched it.

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::Html,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::{SocketAddr, TcpStream},
    path::PathBuf,
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
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

/// The browser control helper is intentionally separate from the sensing
/// HTTP port so it remains reachable while the sensing process is stopped.
pub const DEFAULT_PORT: u16 = 8090;

const STOP_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub server_bin: PathBuf,
    pub server_args: Vec<String>,
    pub http_port: u16,
}

#[derive(Debug, Serialize)]
struct HelperAdoptionRequest {
    server_bin: String,
    server_args: Vec<String>,
    http_port: u16,
    ui_port: u16,
    pid: u32,
}

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub control_port: u16,
    pub ui_port: u16,
    pub spec_path: PathBuf,
    pub adopt_pid: Option<u32>,
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

#[derive(Debug, Serialize)]
struct StatusResponse {
    controller: &'static str,
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

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

/// Start the helper for the current server unless another helper already
/// owns the configured loopback port.
pub fn ensure_daemon(control_port: u16, http_port: u16) -> Result<(), String> {
    if control_port == 0 {
        return Err("Server-Control-Port darf nicht 0 sein".to_string());
    }

    let server_bin = std::env::current_exe().map_err(|error| {
        format!("Pfad der sensing-server-Binary konnte nicht ermittelt werden: {error}")
    })?;
    let server_args = std::env::args().skip(1).collect::<Vec<_>>();
    let spec = LaunchSpec {
        server_bin: server_bin.clone(),
        server_args,
        http_port,
    };
    if control_port_is_listening(control_port) {
        if let Err(error) = adopt_existing_helper(control_port, &spec) {
            warn!(%error, control_port, "Laufender Browser-Control-Helper konnte nicht aktualisiert werden");
        }
        return Ok(());
    }
    let unique_suffix = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let spec_path = std::env::temp_dir().join(format!(
        "ruview-server-control-{}-{unique_suffix}.json",
        std::process::id()
    ));
    let spec_json = serde_json::to_vec(&spec).map_err(|error| {
        format!("Server-Control-Profil konnte nicht serialisiert werden: {error}")
    })?;
    let mut spec_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&spec_path)
        .map_err(|error| {
            format!(
                "Server-Control-Profil konnte nicht angelegt werden ({}): {error}",
                spec_path.display()
            )
        })?;
    std::io::Write::write_all(&mut spec_file, &spec_json).map_err(|error| {
        format!(
            "Server-Control-Profil konnte nicht geschrieben werden ({}): {error}",
            spec_path.display()
        )
    })?;

    let external_helper = server_bin
        .parent()
        .map(|parent| {
            parent.join(if cfg!(windows) {
                "ruview-control.exe"
            } else {
                "ruview-control"
            })
        })
        .filter(|path| path.is_file());
    let mut command =
        std::process::Command::new(external_helper.as_deref().unwrap_or(server_bin.as_path()));
    if external_helper.is_some() {
        command.args([
            "--port",
            &control_port.to_string(),
            "--ui-port",
            &http_port.to_string(),
            "--server-spec",
            spec_path.to_string_lossy().as_ref(),
            "--adopt-pid",
            &std::process::id().to_string(),
        ]);
    } else {
        command.args([
            "--server-control-daemon",
            "--server-control-port",
            &control_port.to_string(),
            "--server-control-ui-port",
            &http_port.to_string(),
            "--server-control-spec",
            spec_path.to_string_lossy().as_ref(),
            "--server-control-adopt-pid",
            &std::process::id().to_string(),
        ]);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // The helper must survive the sensing process it supervises and the
        // terminal session that launched it. `setsid` gives it its own
        // session/process group, while all standard streams are already
        // disconnected above.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    command.spawn().map_err(|error| {
        format!("Browser-Serversteuerung konnte nicht gestartet werden: {error}")
    })?;

    info!(
        control_port,
        external = external_helper.is_some(),
        "Browser server control helper started for sensing-server"
    );
    Ok(())
}

fn adopt_existing_helper(control_port: u16, spec: &LaunchSpec) -> Result<(), String> {
    let url = format!("http://127.0.0.1:{control_port}/adopt");
    let request = HelperAdoptionRequest {
        server_bin: spec.server_bin.to_string_lossy().into_owned(),
        server_args: spec.server_args.clone(),
        http_port: spec.http_port,
        ui_port: spec.http_port,
        pid: std::process::id(),
    };
    ureq::post(&url)
        .timeout(Duration::from_millis(500))
        .send_json(request)
        .map_err(|error| format!("{url} antwortete nicht auf die Helper-Übernahme: {error}"))?;
    Ok(())
}

/// Run the loopback helper mode.  This function only returns when the helper
/// itself is stopped or cannot bind its control port.
pub async fn run_daemon(config: DaemonConfig) -> Result<(), String> {
    let spec_bytes = fs::read(&config.spec_path).map_err(|error| {
        format!(
            "Server-Control-Profil konnte nicht gelesen werden ({}): {error}",
            config.spec_path.display()
        )
    })?;
    let spec: LaunchSpec = serde_json::from_slice(&spec_bytes).map_err(|error| {
        format!(
            "Server-Control-Profil ist ungültig ({}): {error}",
            config.spec_path.display()
        )
    })?;
    if !spec.server_bin.is_file() {
        return Err(format!(
            "Sensing-Server-Binary nicht gefunden: {}",
            spec.server_bin.display()
        ));
    }

    let state = Arc::new(Mutex::new(ControllerState {
        spec,
        ui_port: config.ui_port,
        control_port: config.control_port,
        pid: config.adopt_pid,
        child: None,
        launch_log_path: None,
        busy: false,
        last_action: Some("adopted".to_string()),
        last_error: None,
    }));

    let app = Router::new()
        .route("/", get(control_page))
        .route("/status", get(status))
        .route("/start", post(start))
        .route("/stop", post(stop))
        .route("/restart", post(restart))
        // The helper binds to loopback only. Mutation handlers still enforce
        // the exact local UI/control origins below, while permissive CORS
        // keeps browser preflight handling simple for localhost/127.0.0.1.
        .layer(CorsLayer::permissive())
        .with_state(state);

    let address = SocketAddr::from(([127, 0, 0, 1], config.control_port));
    let listener = TcpListener::bind(address).await.map_err(|error| {
        format!("Server-Control-Port {address} konnte nicht geöffnet werden: {error}")
    })?;
    info!(%address, "Browser server control listening");
    axum::serve(listener, app)
        .await
        .map_err(|error| format!("Browser-Serversteuerung beendet: {error}"))
}

async fn status(State(state): State<SharedController>) -> Json<StatusResponse> {
    let mut state = state.lock().await;
    refresh_process(&mut state);
    Json(status_response(&state))
}

async fn start(
    State(state): State<SharedController>,
    headers: HeaderMap,
) -> Result<Json<ActionResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_mutation_origin(&state, &headers).await?;
    let mut state = begin_action(&state, "start")?;
    refresh_process(&mut state);
    if state.pid.is_some() {
        let response = action_response(
            &state,
            "already_running",
            "Der Sensing-Server läuft bereits.",
        );
        state.busy = false;
        return Ok(Json(response));
    }

    match spawn_server(&mut state) {
        Ok(()) => {
            let response = action_response(&state, "started", "Sensing-Server wurde gestartet.");
            state.busy = false;
            Ok(Json(response))
        }
        Err(error) => {
            state.last_error = Some(error.clone());
            state.busy = false;
            Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, error))
        }
    }
}

async fn stop(
    State(state): State<SharedController>,
    headers: HeaderMap,
) -> Result<Json<ActionResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_mutation_origin(&state, &headers).await?;
    let mut guard = begin_action(&state, "stop")?;
    refresh_process(&mut guard);
    let pid = guard.pid;
    if pid.is_none() {
        guard.busy = false;
        return Ok(Json(action_response(
            &guard,
            "stopped",
            "Sensing-Server läuft nicht.",
        )));
    }
    let pid = pid.expect("pid checked above");
    let signal_error = terminate_process(pid);
    if let Err(error) = signal_error {
        guard.last_error = Some(error.clone());
        guard.busy = false;
        return Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, error));
    }

    drop(guard);
    let stopped = wait_for_stop(&state, pid).await;
    let mut guard = state.lock().await;
    if !stopped {
        guard.last_error = Some(format!(
            "Sensing-Server {pid} reagiert nicht auf das Stoppsignal"
        ));
        guard.busy = false;
        return Err(error_response(
            StatusCode::GATEWAY_TIMEOUT,
            guard.last_error.clone().expect("error just set"),
        ));
    }
    guard.pid = None;
    guard.child = None;
    remove_launch_log(&mut guard);
    guard.last_error = None;
    guard.busy = false;
    Ok(Json(action_response(
        &guard,
        "stopped",
        "Sensing-Server wurde beendet.",
    )))
}

async fn restart(
    State(state): State<SharedController>,
    headers: HeaderMap,
) -> Result<Json<ActionResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_mutation_origin(&state, &headers).await?;
    let mut guard = begin_action(&state, "restart")?;
    refresh_process(&mut guard);
    let pid = guard.pid;
    if let Some(pid) = pid {
        terminate_process(pid).map_err(|error| {
            guard.last_error = Some(error.clone());
            guard.busy = false;
            error_response(StatusCode::INTERNAL_SERVER_ERROR, error)
        })?;
        drop(guard);
        if !wait_for_stop(&state, pid).await {
            let mut guard = state.lock().await;
            guard.last_error = Some(format!(
                "Sensing-Server {pid} reagiert nicht auf den Neustart"
            ));
            guard.busy = false;
            return Err(error_response(
                StatusCode::GATEWAY_TIMEOUT,
                guard.last_error.clone().expect("error just set"),
            ));
        }
        guard = state.lock().await;
        guard.pid = None;
        guard.child = None;
    }

    match spawn_server(&mut guard) {
        Ok(()) => {
            let response =
                action_response(&guard, "restarted", "Sensing-Server wurde neu gestartet.");
            guard.busy = false;
            Ok(Json(response))
        }
        Err(error) => {
            guard.last_error = Some(error.clone());
            guard.busy = false;
            Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, error))
        }
    }
}

fn begin_action<'a>(
    state: &'a SharedController,
    action: &str,
) -> Result<tokio::sync::MutexGuard<'a, ControllerState>, (StatusCode, Json<ErrorResponse>)> {
    // `blocking_lock` is deliberately not used: handlers are async and this
    // short critical section never waits on a process or socket.
    let mut state = state.try_lock().map_err(|_| {
        error_response(
            StatusCode::CONFLICT,
            "Eine andere Serveraktion läuft bereits.".to_string(),
        )
    })?;
    if state.busy {
        return Err(error_response(
            StatusCode::CONFLICT,
            "Eine andere Serveraktion läuft bereits.".to_string(),
        ));
    }
    state.busy = true;
    state.last_action = Some(action.to_string());
    Ok(state)
}

async fn ensure_mutation_origin(
    state: &SharedController,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    if origin.is_none() {
        // Local CLI/curl use remains possible; the listener is loopback-only.
        return Ok(());
    }
    let state = state.lock().await;
    let allowed = [
        format!("http://localhost:{}", state.ui_port),
        format!("http://127.0.0.1:{}", state.ui_port),
        format!("http://[::1]:{}", state.ui_port),
        format!("http://localhost:{}", state.control_port),
        format!("http://127.0.0.1:{}", state.control_port),
        format!("http://[::1]:{}", state.control_port),
    ];
    if allowed
        .iter()
        .any(|candidate| Some(candidate.as_str()) == origin)
    {
        Ok(())
    } else {
        Err(error_response(
            StatusCode::FORBIDDEN,
            "Browser-Origin für die lokale Serversteuerung ist nicht erlaubt.".to_string(),
        ))
    }
}

fn refresh_process(state: &mut ControllerState) {
    if let Some(child) = state.child.as_mut() {
        match child.try_wait() {
            Ok(Some(exit)) => {
                if !exit.success() {
                    let diagnostic = state
                        .launch_log_path
                        .take()
                        .and_then(|path| read_launch_diagnostic(&path));
                    state.last_error = Some(match diagnostic {
                        Some(diagnostic) => {
                            format!("Sensing-Server beendet mit {exit}: {diagnostic}")
                        }
                        None => format!("Sensing-Server beendet mit {exit}"),
                    });
                } else {
                    remove_launch_log(state);
                }
                state.child = None;
                state.pid = None;
            }
            Ok(None) => {}
            Err(error) => {
                remove_launch_log(state);
                state.last_error =
                    Some(format!("Serverstatus konnte nicht gelesen werden: {error}"));
                state.child = None;
                state.pid = None;
            }
        }
    } else if let Some(pid) = state.pid {
        if !process_exists(pid) {
            state.pid = None;
        }
    }
}

fn spawn_server(state: &mut ControllerState) -> Result<(), String> {
    let (log_path, log_file) = create_launch_log()?;
    let mut command = Command::new(&state.spec.server_bin);
    command
        .args(&state.spec.server_args)
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
    state.child = Some(child);
    state.pid = Some(pid);
    state.launch_log_path = Some(log_path);
    state.last_error = None;
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

fn remove_launch_log(state: &mut ControllerState) {
    if let Some(path) = state.launch_log_path.take() {
        let _ = fs::remove_file(path);
    }
}

fn read_launch_diagnostic(path: &PathBuf) -> Option<String> {
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

async fn wait_for_stop(state: &SharedController, pid: u32) -> bool {
    let deadline = tokio::time::Instant::now() + STOP_TIMEOUT;
    loop {
        {
            let mut state = state.lock().await;
            refresh_process(&mut state);
            if state.pid != Some(pid) {
                return true;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        sleep(POLL_INTERVAL).await;
    }

    if process_exists(pid) {
        if let Err(error) = force_kill_process(pid) {
            warn!(pid, %error, "Could not force-stop sensing-server");
            return false;
        }
        // An adopted process cannot be reaped by the helper. Once the OS has
        // accepted SIGKILL, waiting for kill(pid, 0) would misclassify a
        // short-lived zombie as a still-running server.
        return true;
    }
    true
}

fn status_response(state: &ControllerState) -> StatusResponse {
    StatusResponse {
        controller: "ready",
        running: state.pid.is_some(),
        pid: state.pid,
        http_port: state.spec.http_port,
        control_port: state.control_port,
        ui_url: format!(
            "http://localhost:{}/ui/index.html#sensing",
            state.spec.http_port
        ),
        busy: state.busy,
        last_action: state.last_action.clone(),
        error: state.last_error.clone(),
    }
}

fn action_response(state: &ControllerState, status: &'static str, message: &str) -> ActionResponse {
    ActionResponse {
        status,
        running: state.pid.is_some(),
        pid: state.pid,
        message: message.to_string(),
    }
}

fn error_response(status: StatusCode, message: String) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: message }))
}

async fn control_page() -> Html<&'static str> {
    Html(
        r##"<!doctype html>
<html lang="de">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width,initial-scale=1">
    <title>RuView Serversteuerung</title>
    <style>
      :root { color-scheme: dark; font: 16px system-ui, sans-serif; }
      body { max-width: 42rem; margin: 3rem auto; padding: 0 1rem; background: #10131a; color: #eef2f7; }
      main { padding: 1.5rem; border: 1px solid #303846; border-radius: 1rem; background: #181d26; }
      h1 { margin-top: 0; }
      #state { font-weight: 700; }
      .actions { display: flex; flex-wrap: wrap; gap: .75rem; margin: 1.5rem 0 1rem; }
      button { padding: .7rem 1rem; border: 1px solid #56647a; border-radius: .6rem; background: #263246; color: inherit; cursor: pointer; }
      button:hover:not(:disabled) { background: #33445f; }
      button:disabled { cursor: not-allowed; opacity: .45; }
      #message { min-height: 1.5rem; color: #b7c7de; }
      a { color: #9bc6ff; }
    </style>
  </head>
  <body>
    <main>
      <h1>RuView Serversteuerung</h1>
      <p>Diese lokale Browser-Seite bleibt erreichbar, wenn der Sensing-Server beendet ist.</p>
      <p id="state" aria-live="polite">Status wird geladen …</p>
      <div class="actions">
        <button type="button" data-action="start">Server starten</button>
        <button type="button" data-action="restart">Neu starten</button>
        <button type="button" data-action="stop">Server stoppen</button>
        <button type="button" data-action="status">Status prüfen</button>
      </div>
      <p id="message" role="status"></p>
      <p><a href="http://localhost:8080/ui/index.html#sensing">Zur Sensing-Seite</a></p>
      <p><a href="/status">Status als JSON anzeigen</a></p>
    </main>
    <script>
      const stateElement = document.getElementById('state');
      const messageElement = document.getElementById('message');
      const buttons = [...document.querySelectorAll('[data-action]')];

      function setBusy(busy) {
        buttons.forEach((button) => {
          button.disabled = busy;
        });
      }

      async function refresh() {
        try {
          const response = await fetch('/status', { cache: 'no-store' });
          const payload = await response.json();
          stateElement.textContent = payload.running
            ? `Server läuft${payload.pid ? ` · PID ${payload.pid}` : ''}`
            : 'Server ist gestoppt';
          if (payload.error) messageElement.textContent = payload.error;
        } catch (error) {
          stateElement.textContent = 'Control-Helper nicht erreichbar';
          messageElement.textContent = error.message;
        }
      }

      async function runAction(action) {
        if (action === 'status') {
          await refresh();
          return;
        }
        setBusy(true);
        try {
          const response = await fetch(`/${action}`, { method: 'POST' });
          const payload = await response.json();
          if (!response.ok) throw new Error(payload.error || 'Serveraktion fehlgeschlagen');
          messageElement.textContent = payload.message;
          await refresh();
        } catch (error) {
          messageElement.textContent = error.message;
        } finally {
          setBusy(false);
        }
      }

      document.addEventListener('click', (event) => {
        const button = event.target.closest('[data-action]');
        if (button) void runAction(button.dataset.action);
      });
      void refresh();
      window.setInterval(refresh, 3000);
    </script>
  </body>
</html>"##,
    )
}

fn control_port_is_listening(port: u16) -> bool {
    TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(150),
    )
    .is_ok()
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
        "Serversteuerung per Prozesssignal ist auf diesem Betriebssystem noch nicht implementiert"
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
        "Serversteuerung per Prozesssignal ist auf diesem Betriebssystem noch nicht implementiert"
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_port_is_loopback_only_in_the_default_address() {
        let address = SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT));
        assert!(address.ip().is_loopback());
    }

    #[test]
    fn status_uses_the_configured_http_port_for_the_browser_link() {
        let state = ControllerState {
            spec: LaunchSpec {
                server_bin: PathBuf::from("/tmp/sensing-server"),
                server_args: Vec::new(),
                http_port: 8080,
            },
            ui_port: 8080,
            control_port: DEFAULT_PORT,
            pid: None,
            child: None,
            launch_log_path: None,
            busy: false,
            last_action: None,
            last_error: None,
        };

        let status = status_response(&state);
        assert_eq!(status.controller, "ready");
        assert_eq!(status.ui_url, "http://localhost:8080/ui/index.html#sensing");
        assert!(!status.running);
    }
}
