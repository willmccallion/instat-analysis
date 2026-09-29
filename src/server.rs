//! Loopback web server: serves the UI, accepts PDF uploads and answers analysis requests.
//!
//! Every API call must carry the per-launch token, so other web pages the coach has open
//! cannot talk to it.

use std::collections::HashMap;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tiny_http::{Header, Method, Request as HttpRequest, Response, Server, StatusCode};

use crate::analysis::{Request, analyse};
use crate::error::Error;
use crate::model::GameId;
use crate::store::Store;
use crate::web;

const IDLE_SHUTDOWN: Duration = Duration::from_mins(15);
const POLL: Duration = Duration::from_secs(5);
const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;
const TOKEN_HEADER: &str = "X-Hockey-Token";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LockFile {
    port: u16,
    token: String,
}

fn lock_path(data_dir: &Path) -> PathBuf {
    data_dir.join("server.json")
}

/// If another instance is already serving this library, returns its URL.
#[must_use]
pub fn running_instance(data_dir: &Path) -> Option<String> {
    let lock: LockFile = serde_json::from_slice(&std::fs::read(lock_path(data_dir)).ok()?).ok()?;
    let address = SocketAddr::from(([127, 0, 0, 1], lock.port));
    TcpStream::connect_timeout(&address, Duration::from_millis(300)).ok()?;
    Some(format!("http://127.0.0.1:{}/?t={}", lock.port, lock.token))
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
    token: String,
    cache: HashMap<String, String>,
    last_activity: Instant,
    quit: bool,
}

#[derive(Serialize)]
struct StateResponse<'a> {
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

    fn handle(&mut self, mut request: HttpRequest) {
        self.last_activity = Instant::now();
        let url = request.url().to_owned();
        let path = url.split('?').next().unwrap_or_default().to_owned();
        let method = request.method().clone();
        match (&method, path.as_str()) {
            (Method::Get, "/") => respond(request, 200, "text/html; charset=utf-8", web::app_page().into_bytes()),
            (Method::Get, "/favicon.ico") => respond(request, 204, "text/plain", Vec::new()),
            _ if !path.starts_with("/api/") => respond(request, 404, "text/plain", b"not found".to_vec()),
            _ if !has_token(&request, &self.token) => error_json(request, 403, "missing or wrong token"),
            (Method::Get, "/api/state") => {
                let state = StateResponse {
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
            (Method::Post, "/api/heartbeat") => respond_json(request, 200, &true),
            (Method::Post, "/api/quit") => {
                self.quit = true;
                respond_json(request, 200, &true);
            }
            _ => error_json(request, 404, "unknown endpoint"),
        }
    }
}

/// Serves until the coach quits or the page has been gone for [`IDLE_SHUTDOWN`].
/// `on_ready` receives the URL (including the token) once the port is bound.
pub fn serve(data_dir: &Path, store: Store, port: u16, on_ready: impl FnOnce(&str)) -> Result<(), Error> {
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
    };
    std::fs::write(lock_path(data_dir), serde_json::to_vec(&lock)?)?;
    let url = format!("http://127.0.0.1:{bound}/?t={token}");
    on_ready(&url);
    let mut app = App {
        store,
        token,
        cache: HashMap::new(),
        last_activity: Instant::now(),
        quit: false,
    };
    while !app.quit && app.last_activity.elapsed() < IDLE_SHUTDOWN {
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
