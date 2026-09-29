//! Loopback web server: serves the UI, accepts PDF uploads and answers analysis requests.
//!
//! Every API call must carry the per-launch token, so other web pages the coach has open
//! cannot talk to it.

use std::collections::HashMap;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tiny_http::{Header, Method, Request as HttpRequest, Response, Server, StatusCode};

use crate::analysis::rating_setup::RatingWeights;
use crate::analysis::{Request, analyse};
use crate::error::Error;
use crate::model::{GameId, TeamPrefix};
use crate::store::Store;
use crate::update::{self, UpdateStatus};
use crate::web;

const IDLE_SHUTDOWN: Duration = Duration::from_mins(15);
/// How long a closed page has to come back (a reload does within a second) before quitting.
const CLOSE_GRACE: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_secs(1);
const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;
const TOKEN_HEADER: &str = "X-Hockey-Token";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LockFile {
    port: u16,
    token: String,
    /// Fingerprint of the executable serving; absent in locks from older versions.
    #[serde(default)]
    build: String,
}

/// A server already running for this library.
#[derive(Debug, Clone)]
pub enum RunningInstance {
    /// Same executable: reuse it.
    Current(String),
    /// A different (usually older) build: it must be stopped so its stale page isn't shown.
    Outdated(StaleServer),
}

#[derive(Debug, Clone)]
pub struct StaleServer {
    port: u16,
    token: String,
}

const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Identifies this executable, so a relaunch after an update replaces the old server.
pub fn build_id() -> Result<String, Error> {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(&std::fs::read(std::env::current_exe()?)?);
    Ok(format!("{:016x}", hasher.finish()))
}

fn lock_path(data_dir: &Path) -> PathBuf {
    data_dir.join("server.json")
}

fn is_listening(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok()
}

/// The server already serving this library, if any, and whether it is this build.
#[must_use]
pub fn running_instance(data_dir: &Path, build: &str) -> Option<RunningInstance> {
    let lock: LockFile = serde_json::from_slice(&std::fs::read(lock_path(data_dir)).ok()?).ok()?;
    if !is_listening(lock.port) {
        return None;
    }
    Some(if lock.build == build {
        RunningInstance::Current(format!("http://127.0.0.1:{}/?t={}", lock.port, lock.token))
    } else {
        RunningInstance::Outdated(StaleServer {
            port: lock.port,
            token: lock.token,
        })
    })
}

/// Asks an outdated server to quit and waits until its port is free.
pub fn stop(server: &StaleServer) -> Result<(), Error> {
    use std::io::Write;
    let mut stream = TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], server.port)), STOP_TIMEOUT)?;
    stream.set_read_timeout(Some(STOP_TIMEOUT))?;
    write!(
        stream,
        "POST /api/quit HTTP/1.1\r\nHost: 127.0.0.1\r\n{TOKEN_HEADER}: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        server.token
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    if !response.starts_with("HTTP/1.1 200") {
        return Err(Error::Io(std::io::Error::other(format!(
            "the running copy refused to quit: {}",
            response.lines().next().unwrap_or_default()
        ))));
    }
    let started = Instant::now();
    while is_listening(server.port) {
        if started.elapsed() > STOP_TIMEOUT {
            return Err(Error::Io(std::io::Error::other(
                "an older copy is still running; use Quit app in its page, then open the app again",
            )));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

/// 256 bits from the OS-seeded generator, hex-encoded.
fn new_token() -> String {
    use rand::RngExt;
    use std::fmt::Write;
    let mut rng = rand::rng();
    let mut token = String::with_capacity(64);
    for _ in 0..4 {
        // Writing into a String cannot fail.
        let _ = write!(token, "{:016x}", rng.random::<u64>());
    }
    token
}

struct App {
    store: Store,
    update: Arc<Mutex<UpdateStatus>>,
    token: String,
    cache: HashMap<String, String>,
    last_activity: Instant,
    /// When a page last reported it was closing, if nothing has been heard since.
    closing_since: Option<Instant>,
    quit: bool,
}

#[derive(Serialize)]
struct StateResponse<'a> {
    team: Option<&'a TeamPrefix>,
    rating_weights: &'a RatingWeights,
    games: usize,
    pending: Vec<String>,
    problems: &'a [String],
}

fn header(name: &str, value: &str) -> Option<Header> {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).ok()
}

fn respond(request: HttpRequest, status: u16, content_type: &str, body: Vec<u8>) {
    let mut response = Response::from_data(body).with_status_code(StatusCode(status));
    for h in [
        header("Content-Type", content_type),
        header("Cache-Control", "no-store"),
        header("X-Content-Type-Options", "nosniff"),
    ]
    .into_iter()
    .flatten()
    {
        response.add_header(h);
    }
    if let Err(e) = request.respond(response) {
        eprintln!("failed to send response: {e}");
    }
}

fn respond_json<T: Serialize>(request: HttpRequest, status: u16, value: &T) {
    match serde_json::to_vec(value) {
        Ok(body) => respond(request, status, "application/json", body),
        Err(e) => respond(request, 500, "text/plain", e.to_string().into_bytes()),
    }
}

#[derive(Serialize)]
struct Message {
    error: String,
}

fn error_json(request: HttpRequest, status: u16, message: impl Into<String>) {
    respond_json(request, status, &Message { error: message.into() });
}

fn read_body(request: &mut HttpRequest) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    request
        .as_reader()
        .take(MAX_UPLOAD_BYTES as u64 + 1)
        .read_to_end(&mut body)?;
    if body.len() > MAX_UPLOAD_BYTES {
        return Err(Error::Pdf("file too large".into()));
    }
    Ok(body)
}

fn has_token(request: &HttpRequest, token: &str) -> bool {
    request
        .headers()
        .iter()
        .any(|h| h.field.equiv(TOKEN_HEADER) && h.value.as_str() == token)
}

impl App {
    fn analysis_json(&mut self, request: &Request) -> Result<String, Error> {
        let key = serde_json::to_string(request)?;
        if let Some(cached) = self.cache.get(&key) {
            return Ok(cached.clone());
        }
        let games = self.store.games();
        let json = serde_json::to_string(&analyse(&games, request))?;
        self.cache.insert(key, json.clone());
        Ok(json)
    }

    fn update_status(&self) -> UpdateStatus {
        self.update.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn should_stop(&self) -> bool {
        self.quit
            || self.last_activity.elapsed() >= IDLE_SHUTDOWN
            || self.closing_since.is_some_and(|t| t.elapsed() >= CLOSE_GRACE)
    }

    fn handle(&mut self, mut request: HttpRequest) {
        self.last_activity = Instant::now();
        let url = request.url().to_owned();
        let path = url.split('?').next().unwrap_or_default().to_owned();
        if path != "/api/closing" {
            self.closing_since = None;
        }
        let method = request.method().clone();
        match (&method, path.as_str()) {
            (Method::Get, "/") => respond(request, 200, "text/html; charset=utf-8", web::app_page().into_bytes()),
            (Method::Get, "/favicon.ico") => respond(request, 204, "text/plain", Vec::new()),
            _ if !path.starts_with("/api/") => respond(request, 404, "text/plain", b"not found".to_vec()),
            _ if !has_token(&request, &self.token) => error_json(request, 403, "missing or wrong token"),
            (Method::Get, "/api/state") => {
                let state = StateResponse {
                    team: self.store.team(),
                    rating_weights: self.store.rating_weights(),
                    games: self.store.games().len(),
                    pending: self.store.pending_descriptions(),
                    problems: &self.store.problems,
                };
                respond_json(request, 200, &state);
            }
            (Method::Post, "/api/upload") => match read_body(&mut request) {
                Ok(body) => match self.store.add_pdf(&body) {
                    Ok(outcome) => {
                        self.cache.clear();
                        respond_json(request, 200, &outcome);
                    }
                    Err(e) => error_json(request, 422, e.to_string()),
                },
                Err(e) => error_json(request, 413, e.to_string()),
            },
            (Method::Post, "/api/analyze") => {
                let parsed = read_body(&mut request)
                    .and_then(|body| serde_json::from_slice::<Request>(&body).map_err(Error::from));
                match parsed.and_then(|r| self.analysis_json(&r)) {
                    Ok(json) => respond(request, 200, "application/json", json.into_bytes()),
                    Err(e) => error_json(request, 400, e.to_string()),
                }
            }
            (Method::Post, "/api/export") => {
                let parsed = read_body(&mut request)
                    .and_then(|body| serde_json::from_slice::<Request>(&body).map_err(Error::from));
                match parsed.and_then(|r| self.analysis_json(&r)) {
                    Ok(json) => respond(request, 200, "text/html; charset=utf-8", web::snapshot_page(&json).into_bytes()),
                    Err(e) => error_json(request, 400, e.to_string()),
                }
            }
            (Method::Delete, games_path) if games_path.starts_with("/api/games/") => {
                let id = GameId(games_path.trim_start_matches("/api/games/").to_owned());
                match self.store.remove_game(&id) {
                    Ok(true) => {
                        self.cache.clear();
                        respond_json(request, 200, &true);
                    }
                    Ok(false) => error_json(request, 404, "no such game"),
                    Err(e) => error_json(request, 500, e.to_string()),
                }
            }
            (Method::Post, "/api/team") => {
                let parsed = read_body(&mut request).and_then(|body| {
                    let text = String::from_utf8(body).map_err(|_| Error::parse("team", "not text"))?;
                    TeamPrefix::parse(&text).ok_or_else(|| Error::parse("team", "team name is empty"))
                });
                match parsed.and_then(|team| self.store.set_team(team)) {
                    Ok(()) => {
                        self.cache.clear();
                        respond_json(request, 200, &self.store.team());
                    }
                    Err(e) => error_json(request, 400, e.to_string()),
                }
            }
            (Method::Post, "/api/rating-weights") => {
                let parsed = read_body(&mut request)
                    .and_then(|body| serde_json::from_slice::<RatingWeights>(&body).map_err(Error::from));
                match parsed.and_then(|weights| self.store.set_rating_weights(weights)) {
                    Ok(()) => respond_json(request, 200, self.store.rating_weights()),
                    Err(e) => error_json(request, 400, e.to_string()),
                }
            }
            (Method::Get, "/api/update") => respond_json(request, 200, &self.update_status()),
            (Method::Post, "/api/update") => match self.update_status() {
                UpdateStatus::Available { release } => match update::install(&release) {
                    Ok(()) => respond_json(request, 200, &release.version),
                    Err(e) => error_json(request, 500, e.to_string()),
                },
                _ => error_json(request, 409, "no update is available"),
            },
            (Method::Post, "/api/heartbeat") => respond_json(request, 200, &true),
            (Method::Post, "/api/closing") => {
                self.closing_since = Some(Instant::now());
                respond_json(request, 200, &true);
            }
            (Method::Post, "/api/quit") => {
                self.quit = true;
                respond_json(request, 200, &true);
            }
            _ => error_json(request, 404, "unknown endpoint"),
        }
    }
}

/// Serves until the coach quits, the page closes (see [`CLOSE_GRACE`]) or nothing has been
/// heard for [`IDLE_SHUTDOWN`].
/// `on_ready` receives the URL (including the token) once the port is bound.
pub fn serve(data_dir: &Path, store: Store, port: u16, build: String, on_ready: impl FnOnce(&str)) -> Result<(), Error> {
    let server = Server::http(("127.0.0.1", port)).map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
    let bound = server
        .server_addr()
        .to_ip()
        .map(|a| a.port())
        .ok_or_else(|| Error::Io(std::io::Error::other("server is not on an IP socket")))?;
    let token = new_token();
    let lock = LockFile {
        port: bound,
        token: token.clone(),
        build,
    };
    std::fs::write(lock_path(data_dir), serde_json::to_vec(&lock)?)?;
    let url = format!("http://127.0.0.1:{bound}/?t={token}");
    on_ready(&url);
    let update = Arc::new(Mutex::new(UpdateStatus::Checking));
    let checker = Arc::clone(&update);
    std::thread::spawn(move || {
        let status = update::check();
        *checker.lock().unwrap_or_else(PoisonError::into_inner) = status;
    });
    let mut app = App {
        store,
        update,
        token,
        cache: HashMap::new(),
        last_activity: Instant::now(),
        closing_since: None,
        quit: false,
    };
    while !app.should_stop() {
        match server.recv_timeout(POLL) {
            Ok(Some(request)) => app.handle(request),
            Ok(None) => {}
            Err(e) => eprintln!("connection error: {e}"),
        }
    }
    if let Err(e) = std::fs::remove_file(lock_path(data_dir)) {
        eprintln!("could not remove lock file: {e}");
    }
    Ok(())
}

/// Opens `url` in the default browser.
pub fn open_browser(url: &str) -> Result<(), Error> {
    let status = if cfg!(target_os = "macos") {
        std::process::Command::new("/usr/bin/open").arg(url).status()?
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd").args(["/C", "start", "", url]).status()?
    } else {
        std::process::Command::new("xdg-open").arg(url).status()?
    };
    if status.success() {
        Ok(())
    } else {
        Err(Error::Io(std::io::Error::other(format!("browser launcher exited with {status}"))))
    }
}
