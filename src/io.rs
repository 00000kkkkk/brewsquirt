use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

pub type Headers = Vec<(String, String)>;

pub struct FetchRequest {
    pub method: String,
    pub url: String,
    pub headers: Headers,
    pub body: Option<Vec<u8>>,
    pub timeout: Option<Duration>,
    pub follow: bool,
}

pub struct FetchResponse {
    pub status: u16,
    pub status_text: String,
    pub url: String,
    pub headers: Headers,
    pub body: Vec<u8>,
}

pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub remote: String,
}

pub struct HttpReply {
    pub status: u16,
    pub headers: Headers,
    pub body: Vec<u8>,
}

pub enum IoEvent {
    Fetch {
        id: u64,
        result: Result<FetchResponse, String>,
    },
    Request {
        server: u64,
        request: HttpRequest,
        reply: Sender<HttpReply>,
    },
}

pub struct Io {
    pub tx: Sender<IoEvent>,
    pub rx: Receiver<IoEvent>,
    pub pending: usize,
    pub seq: u64,
}

impl Default for Io {
    fn default() -> Self {
        let (tx, rx) = channel();
        Io {
            tx,
            rx,
            pending: 0,
            seq: 0,
        }
    }
}

impl Io {
    pub fn next_id(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .user_agent(format!("javasquirt/{}", crate::VERSION))
            .build()
            .into()
    })
}

pub fn status_text(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        418 => "I'm a Teapot",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

fn run_fetch(req: FetchRequest) -> Result<FetchResponse, String> {
    use ureq::RequestExt;
    let mut builder = ureq::http::Request::builder()
        .method(req.method.as_str())
        .uri(req.url.as_str());
    for (k, v) in &req.headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    let redirects = if req.follow { 20 } else { 0 };
    let mut resp = match req.body {
        Some(body) => builder
            .body(body)
            .map_err(|e| e.to_string())?
            .with_agent(agent())
            .configure()
            .max_redirects(redirects)
            .max_redirects_will_error(false)
            .timeout_global(req.timeout)
            .allow_non_standard_methods(true)
            .build()
            .run(),
        None => builder
            .body(())
            .map_err(|e| e.to_string())?
            .with_agent(agent())
            .configure()
            .max_redirects(redirects)
            .max_redirects_will_error(false)
            .timeout_global(req.timeout)
            .allow_non_standard_methods(true)
            .build()
            .run(),
    }
    .map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let url = ureq::ResponseExt::get_uri(&resp).to_string();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let body = resp
        .body_mut()
        .with_config()
        .limit(u64::MAX)
        .read_to_vec()
        .map_err(|e| e.to_string())?;
    Ok(FetchResponse {
        status,
        status_text: status_text(status).to_string(),
        url,
        headers,
        body,
    })
}

pub fn spawn_fetch(tx: Sender<IoEvent>, id: u64, req: FetchRequest) {
    std::thread::spawn(move || {
        let result = run_fetch(req);
        let _ = tx.send(IoEvent::Fetch { id, result });
    });
}

pub struct ServerHandle {
    pub addr: SocketAddr,
    stop: Arc<AtomicBool>,
}

impl ServerHandle {
    pub fn close(&self) {
        if self.stop.swap(true, Ordering::SeqCst) {
            return;
        }
        let mut wake = self.addr;
        if wake.ip().is_unspecified() {
            wake.set_ip(if wake.is_ipv4() {
                std::net::Ipv4Addr::LOCALHOST.into()
            } else {
                std::net::Ipv6Addr::LOCALHOST.into()
            });
        }
        let _ = TcpStream::connect_timeout(&wake, Duration::from_millis(200));
    }
}

pub fn listen(
    host: &str,
    port: u16,
    server: u64,
    tx: Sender<IoEvent>,
) -> std::io::Result<ServerHandle> {
    let listener = TcpListener::bind((host, port))?;
    let addr = listener.local_addr()?;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            if flag.load(Ordering::SeqCst) {
                break;
            }
            let Ok(stream) = conn else { continue };
            let tx = tx.clone();
            let flag = flag.clone();
            std::thread::spawn(move || serve_connection(stream, server, tx, flag));
        }
    });
    Ok(ServerHandle { addr, stop })
}

const MAX_HEAD: usize = 64 * 1024;

fn read_line(r: &mut impl BufRead, budget: &mut usize) -> Option<String> {
    let mut buf = Vec::new();
    let n = r
        .by_ref()
        .take(*budget as u64 + 1)
        .read_until(b'\n', &mut buf)
        .ok()?;
    if n == 0 || n > *budget {
        return None;
    }
    *budget -= n;
    while matches!(buf.last(), Some(b'\n' | b'\r')) {
        buf.pop();
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn read_chunked(r: &mut impl BufRead) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let mut budget = MAX_HEAD;
        let line = read_line(r, &mut budget)?;
        let size = usize::from_str_radix(line.split(';').next()?.trim(), 16).ok()?;
        if size == 0 {
            loop {
                let mut budget = MAX_HEAD;
                if read_line(r, &mut budget)?.is_empty() {
                    return Some(body);
                }
            }
        }
        let start = body.len();
        body.resize(start + size, 0);
        r.read_exact(&mut body[start..]).ok()?;
        let mut budget = 2;
        read_line(r, &mut budget)?;
    }
}

struct Parsed {
    request: HttpRequest,
    keep_alive: bool,
    head: bool,
}

fn read_request(
    r: &mut BufReader<TcpStream>,
    local: &SocketAddr,
    remote: &SocketAddr,
) -> Option<Parsed> {
    let mut budget = MAX_HEAD;
    let mut line = read_line(r, &mut budget)?;
    while line.is_empty() {
        line = read_line(r, &mut budget)?;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let version = parts.next().unwrap_or("HTTP/1.0").to_string();
    let mut headers = Vec::new();
    loop {
        let h = read_line(r, &mut budget)?;
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    let find = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.to_ascii_lowercase())
    };
    let connection = find("connection").unwrap_or_default();
    let keep_alive = if version == "HTTP/1.0" {
        connection.contains("keep-alive")
    } else {
        !connection.contains("close")
    };
    let body = if find("transfer-encoding").is_some_and(|t| t.contains("chunked")) {
        read_chunked(r)?
    } else {
        let len: usize = find("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0; len];
        r.read_exact(&mut body).ok()?;
        body
    };
    let host = headers
        .iter()
        .find(|(k, _)| k == "host")
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| local.to_string());
    let url = if target.starts_with("http://") || target.starts_with("https://") {
        target
    } else {
        format!("http://{host}{target}")
    };
    let head = method == "HEAD";
    Some(Parsed {
        request: HttpRequest {
            method,
            url,
            headers,
            body,
            remote: remote.ip().to_string(),
        },
        keep_alive,
        head,
    })
}

fn http_date() -> String {
    chrono::Utc::now()
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string()
}

fn write_reply(
    w: &mut TcpStream,
    reply: &HttpReply,
    keep_alive: bool,
    head: bool,
) -> std::io::Result<()> {
    let mut out = Vec::with_capacity(256 + reply.body.len());
    let _ = write!(
        out,
        "HTTP/1.1 {} {}\r\n",
        reply.status,
        status_text(reply.status)
    );
    let mut has_date = false;
    for (k, v) in &reply.headers {
        let lower = k.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "content-length" | "transfer-encoding" | "connection" | "keep-alive"
        ) {
            continue;
        }
        has_date |= lower == "date";
        let _ = write!(out, "{k}: {v}\r\n");
    }
    if !has_date {
        let _ = write!(out, "date: {}\r\n", http_date());
    }
    let no_body = reply.status == 204 || reply.status == 304 || (100..200).contains(&reply.status);
    if !no_body {
        let _ = write!(out, "content-length: {}\r\n", reply.body.len());
    }
    if keep_alive {
        out.extend_from_slice(b"connection: keep-alive\r\nkeep-alive: timeout=5\r\n\r\n");
    } else {
        out.extend_from_slice(b"connection: close\r\n\r\n");
    }
    if !head && !no_body {
        out.extend_from_slice(&reply.body);
    }
    w.write_all(&out)?;
    w.flush()
}

fn serve_connection(stream: TcpStream, server: u64, tx: Sender<IoEvent>, stop: Arc<AtomicBool>) {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let (Ok(local), Ok(remote)) = (stream.local_addr(), stream.peer_addr()) else {
        return;
    };
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    while !stop.load(Ordering::SeqCst) {
        let Some(parsed) = read_request(&mut reader, &local, &remote) else {
            return;
        };
        let (reply_tx, reply_rx) = channel();
        if tx
            .send(IoEvent::Request {
                server,
                request: parsed.request,
                reply: reply_tx,
            })
            .is_err()
        {
            return;
        }
        let Ok(reply) = reply_rx.recv() else {
            return;
        };
        let keep_alive = parsed.keep_alive && !stop.load(Ordering::SeqCst);
        if write_reply(&mut writer, &reply, keep_alive, parsed.head).is_err() || !keep_alive {
            return;
        }
    }
}
