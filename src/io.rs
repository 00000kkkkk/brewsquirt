use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    Stdin(Option<Vec<u8>>),
    Socket {
        id: u64,
        event: SocketEvent,
    },
    Accept {
        server: u64,
        id: u64,
        local: SocketAddr,
        remote: SocketAddr,
        cmd: Sender<SocketCmd>,
    },
    Child {
        id: u64,
        event: ChildEvent,
    },
    Upgrade {
        server: u64,
        id: u64,
        request: HttpRequest,
        local: SocketAddr,
        remote: SocketAddr,
        cmd: Sender<SocketCmd>,
        head: Vec<u8>,
    },
}

pub enum SocketEvent {
    Connected {
        local: SocketAddr,
        remote: SocketAddr,
    },
    Data(Vec<u8>),
    End,
    Error {
        code: String,
        message: String,
    },
    Closed,
}

pub enum SocketCmd {
    Write(Vec<u8>),
    End,
    Destroy,
    Flush(Sender<()>),
}

pub enum ChildEvent {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Exit {
        code: Option<i32>,
        signal: Option<String>,
    },
    Error(String),
}

pub enum ChildCmd {
    Write(Vec<u8>),
    EndStdin,
    Kill(i32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StdioMode {
    Pipe,
    Inherit,
    Null,
}

pub struct ChildSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Option<Vec<(String, String)>>,
    pub shell: bool,
    pub stdin: StdioMode,
    pub stdout: StdioMode,
    pub stderr: StdioMode,
}

pub struct Io {
    pub tx: Sender<IoEvent>,
    pub rx: Receiver<IoEvent>,
    pub pending: usize,
    pub ids: Arc<AtomicU64>,
}

impl Default for Io {
    fn default() -> Self {
        let (tx, rx) = channel();
        Io {
            tx,
            rx,
            pending: 0,
            ids: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl Io {
    pub fn next_id(&mut self) -> u64 {
        self.ids.fetch_add(1, Ordering::SeqCst) + 1
    }
}

pub fn error_code(e: &std::io::Error) -> &'static str {
    use std::io::ErrorKind::*;
    match e.kind() {
        ConnectionRefused => "ECONNREFUSED",
        ConnectionReset => "ECONNRESET",
        ConnectionAborted => "ECONNABORTED",
        TimedOut => "ETIMEDOUT",
        BrokenPipe => "EPIPE",
        NotConnected => "ENOTCONN",
        AddrInUse => "EADDRINUSE",
        AddrNotAvailable => "EADDRNOTAVAIL",
        HostUnreachable | NetworkUnreachable => "EHOSTUNREACH",
        PermissionDenied => "EACCES",
        _ => "EIO",
    }
}

fn tls_config() -> Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let roots = rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            };
            let config = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .expect("tls versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
            Arc::new(config)
        })
        .clone()
}

fn tls_wrap(
    tcp: TcpStream,
    servername: &str,
) -> Result<rustls::StreamOwned<rustls::ClientConnection, TcpStream>, String> {
    let name = rustls::pki_types::ServerName::try_from(servername.to_string())
        .map_err(|e| format!("invalid server name {servername}: {e}"))?;
    let conn = rustls::ClientConnection::new(tls_config(), name).map_err(|e| e.to_string())?;
    let mut stream = rustls::StreamOwned::new(conn, tcp);
    while stream.conn.is_handshaking() {
        stream
            .conn
            .complete_io(&mut stream.sock)
            .map_err(|e| e.to_string())?;
    }
    Ok(stream)
}

enum Flow {
    Continue,
    Stop,
}

fn socket_cmd<S: Write>(
    cmd: SocketCmd,
    tcp: &TcpStream,
    stream: &mut S,
    write_done: &mut bool,
    read_done: bool,
    id: u64,
    tx: &Sender<IoEvent>,
) -> Flow {
    match cmd {
        SocketCmd::Write(data) => {
            if let Err(e) = stream.write_all(&data).and_then(|_| stream.flush()) {
                let _ = tx.send(IoEvent::Socket {
                    id,
                    event: SocketEvent::Error {
                        code: error_code(&e).to_string(),
                        message: format!("write {}", error_code(&e)),
                    },
                });
                return Flow::Stop;
            }
            Flow::Continue
        }
        SocketCmd::End => {
            let _ = stream.flush();
            let _ = tcp.shutdown(std::net::Shutdown::Write);
            *write_done = true;
            if read_done {
                Flow::Stop
            } else {
                Flow::Continue
            }
        }
        SocketCmd::Flush(ack) => {
            let _ = stream.flush();
            let _ = ack.send(());
            Flow::Continue
        }
        SocketCmd::Destroy => Flow::Stop,
    }
}

fn socket_loop<S: Read + Write>(
    tcp: TcpStream,
    mut stream: S,
    id: u64,
    rx: Receiver<SocketCmd>,
    tx: Sender<IoEvent>,
) {
    let mut buf = vec![0u8; 65536];
    let mut read_done = false;
    let mut write_done = false;
    'outer: loop {
        if read_done {
            match rx.recv() {
                Ok(cmd) => {
                    if let Flow::Stop =
                        socket_cmd(cmd, &tcp, &mut stream, &mut write_done, read_done, id, &tx)
                    {
                        break 'outer;
                    }
                    if write_done {
                        break 'outer;
                    }
                    continue;
                }
                Err(_) => break 'outer,
            }
        }
        loop {
            match rx.try_recv() {
                Ok(cmd) => {
                    if let Flow::Stop =
                        socket_cmd(cmd, &tcp, &mut stream, &mut write_done, read_done, id, &tx)
                    {
                        break 'outer;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break 'outer,
            }
        }
        match stream.read(&mut buf) {
            Ok(0) => {
                read_done = true;
                let _ = tx.send(IoEvent::Socket {
                    id,
                    event: SocketEvent::End,
                });
                if write_done {
                    break 'outer;
                }
            }
            Ok(n) => {
                let _ = tx.send(IoEvent::Socket {
                    id,
                    event: SocketEvent::Data(buf[..n].to_vec()),
                });
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(e) => {
                if !matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::UnexpectedEof
                ) {
                    let code = error_code(&e);
                    let _ = tx.send(IoEvent::Socket {
                        id,
                        event: SocketEvent::Error {
                            code: code.to_string(),
                            message: format!("read {code}"),
                        },
                    });
                } else if !read_done {
                    let _ = tx.send(IoEvent::Socket {
                        id,
                        event: SocketEvent::End,
                    });
                }
                break 'outer;
            }
        }
    }
    let _ = tcp.shutdown(std::net::Shutdown::Both);
    let _ = tx.send(IoEvent::Socket {
        id,
        event: SocketEvent::Closed,
    });
}

pub fn socket_connect(
    id: u64,
    host: String,
    port: u16,
    tls: Option<String>,
    tx: Sender<IoEvent>,
) -> Sender<SocketCmd> {
    let (ctx, crx) = channel::<SocketCmd>();
    std::thread::spawn(move || {
        let fail = |code: &str, message: String| {
            let _ = tx.send(IoEvent::Socket {
                id,
                event: SocketEvent::Error {
                    code: code.to_string(),
                    message,
                },
            });
            let _ = tx.send(IoEvent::Socket {
                id,
                event: SocketEvent::Closed,
            });
        };
        let addrs: Vec<SocketAddr> =
            match std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port)) {
                Ok(a) => a.collect(),
                Err(_) => {
                    fail("ENOTFOUND", format!("getaddrinfo ENOTFOUND {host}"));
                    return;
                }
            };
        if addrs.is_empty() {
            fail("ENOTFOUND", format!("getaddrinfo ENOTFOUND {host}"));
            return;
        }
        let mut stream = None;
        let mut last: Option<std::io::Error> = None;
        for a in &addrs {
            match TcpStream::connect_timeout(a, Duration::from_secs(60)) {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(e) => last = Some(e),
            }
        }
        let Some(stream) = stream else {
            let e = last.unwrap_or_else(|| std::io::Error::other("connect failed"));
            let code = error_code(&e);
            fail(code, format!("connect {code} {host}:{port}"));
            return;
        };
        let _ = stream.set_nodelay(true);
        let (Ok(local), Ok(remote)) = (stream.local_addr(), stream.peer_addr()) else {
            fail("EIO", "connect failed".to_string());
            return;
        };
        let Ok(tcp) = stream.try_clone() else {
            fail("EIO", "connect failed".to_string());
            return;
        };
        match tls {
            None => {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(20)));
                let _ = tx.send(IoEvent::Socket {
                    id,
                    event: SocketEvent::Connected { local, remote },
                });
                socket_loop(tcp, stream, id, crx, tx);
            }
            Some(servername) => match tls_wrap(stream, &servername) {
                Ok(secure) => {
                    let _ = secure
                        .sock
                        .set_read_timeout(Some(Duration::from_millis(20)));
                    let _ = tx.send(IoEvent::Socket {
                        id,
                        event: SocketEvent::Connected { local, remote },
                    });
                    socket_loop(tcp, secure, id, crx, tx);
                }
                Err(message) => fail("ERR_TLS_HANDSHAKE", message),
            },
        }
    });
    ctx
}

pub fn socket_listen(
    host: &str,
    port: u16,
    server: u64,
    ids: Arc<AtomicU64>,
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
            let (Ok(local), Ok(remote)) = (stream.local_addr(), stream.peer_addr()) else {
                continue;
            };
            let Ok(tcp) = stream.try_clone() else {
                continue;
            };
            let _ = stream.set_nodelay(true);
            let _ = stream.set_read_timeout(Some(Duration::from_millis(20)));
            let id = ids.fetch_add(1, Ordering::SeqCst) + 1;
            let (ctx, crx) = channel::<SocketCmd>();
            let tx2 = tx.clone();
            let _ = tx.send(IoEvent::Accept {
                server,
                id,
                local,
                remote,
                cmd: ctx,
            });
            std::thread::spawn(move || socket_loop(tcp, stream, id, crx, tx2));
        }
    });
    Ok(ServerHandle { addr, stop })
}

#[cfg(windows)]
fn shell_for(line: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut c = std::process::Command::new("cmd");
    c.args(["/d", "/s", "/c"]);
    c.raw_arg(format!("\"{line}\""));
    c
}

#[cfg(not(windows))]
fn shell_for(line: &str) -> std::process::Command {
    let mut c = std::process::Command::new("sh");
    c.arg("-c").arg(line);
    c
}

fn stdio_for(mode: StdioMode) -> std::process::Stdio {
    match mode {
        StdioMode::Pipe => std::process::Stdio::piped(),
        StdioMode::Inherit => std::process::Stdio::inherit(),
        StdioMode::Null => std::process::Stdio::null(),
    }
}

pub fn signal_label(signal: i32) -> String {
    match signal {
        1 => "SIGHUP",
        2 => "SIGINT",
        3 => "SIGQUIT",
        6 => "SIGABRT",
        9 => "SIGKILL",
        10 => "SIGUSR1",
        12 => "SIGUSR2",
        13 => "SIGPIPE",
        14 => "SIGALRM",
        15 => "SIGTERM",
        _ => "SIGTERM",
    }
    .to_string()
}

pub fn signal_number(name: &str) -> Option<i32> {
    Some(match name {
        "SIGHUP" => 1,
        "SIGINT" => 2,
        "SIGQUIT" => 3,
        "SIGABRT" => 6,
        "SIGKILL" => 9,
        "SIGUSR1" => 10,
        "SIGUSR2" => 12,
        "SIGPIPE" => 13,
        "SIGALRM" => 14,
        "SIGTERM" => 15,
        _ => return None,
    })
}

#[cfg(unix)]
fn send_signal(child: &mut std::process::Child, signal: i32) {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    if unsafe { kill(child.id() as i32, signal) } != 0 {
        let _ = child.kill();
    }
}

#[cfg(not(unix))]
fn send_signal(child: &mut std::process::Child, _signal: i32) {
    let _ = child.kill();
}

#[cfg(unix)]
fn signal_name(status: &std::process::ExitStatus) -> Option<String> {
    use std::os::unix::process::ExitStatusExt;
    status.signal().map(signal_label)
}

#[cfg(not(unix))]
fn signal_name(_status: &std::process::ExitStatus) -> Option<String> {
    None
}

pub fn child_spawn(
    id: u64,
    spec: ChildSpec,
    tx: Sender<IoEvent>,
) -> std::io::Result<(Sender<ChildCmd>, u32)> {
    let mut cmd = if spec.shell {
        let mut line = spec.program.clone();
        for a in &spec.args {
            line.push(' ');
            line.push_str(a);
        }
        shell_for(&line)
    } else {
        let mut c = std::process::Command::new(&spec.program);
        c.args(&spec.args);
        c
    };
    if let Some(dir) = &spec.cwd {
        cmd.current_dir(dir);
    }
    if let Some(env) = &spec.env {
        cmd.env_clear();
        for (k, v) in env {
            cmd.env(k, v);
        }
    }
    cmd.stdin(stdio_for(spec.stdin));
    cmd.stdout(stdio_for(spec.stdout));
    cmd.stderr(stdio_for(spec.stderr));
    let mut child = cmd.spawn()?;
    let pid = child.id();
    let (ctx, crx) = channel::<ChildCmd>();
    let killed = Arc::new(AtomicBool::new(false));
    let kill_signal = Arc::new(std::sync::atomic::AtomicI32::new(15));
    let mut stdin = child.stdin.take();
    let kill_flag = killed.clone();
    let kill_sig = kill_signal.clone();
    std::thread::spawn(move || {
        while let Ok(cmd) = crx.recv() {
            match cmd {
                ChildCmd::Write(data) => {
                    if let Some(s) = stdin.as_mut() {
                        if s.write_all(&data).and_then(|_| s.flush()).is_err() {
                            stdin = None;
                        }
                    }
                }
                ChildCmd::EndStdin => {
                    stdin = None;
                }
                ChildCmd::Kill(signal) => {
                    kill_sig.store(signal, Ordering::SeqCst);
                    kill_flag.store(true, Ordering::SeqCst);
                }
            }
        }
    });
    if let Some(mut out) = child.stdout.take() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            while let Ok(n) = out.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let _ = tx.send(IoEvent::Child {
                    id,
                    event: ChildEvent::Stdout(buf[..n].to_vec()),
                });
            }
        });
    }
    if let Some(mut err) = child.stderr.take() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            while let Ok(n) = err.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let _ = tx.send(IoEvent::Child {
                    id,
                    event: ChildEvent::Stderr(buf[..n].to_vec()),
                });
            }
        });
    }
    std::thread::spawn(move || {
        let mut was_killed = false;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code();
                    let signal = signal_name(&status).or_else(|| {
                        was_killed.then(|| signal_label(kill_signal.load(Ordering::SeqCst)))
                    });
                    let code = if signal.is_some() { None } else { code };
                    let _ = tx.send(IoEvent::Child {
                        id,
                        event: ChildEvent::Exit { code, signal },
                    });
                    break;
                }
                Ok(None) => {
                    if killed.load(Ordering::SeqCst) && !was_killed {
                        was_killed = true;
                        send_signal(&mut child, kill_signal.load(Ordering::SeqCst));
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => {
                    let _ = tx.send(IoEvent::Child {
                        id,
                        event: ChildEvent::Error(e.to_string()),
                    });
                    let _ = tx.send(IoEvent::Child {
                        id,
                        event: ChildEvent::Exit {
                            code: None,
                            signal: None,
                        },
                    });
                    break;
                }
            }
        }
    });
    Ok((ctx, pid))
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

pub fn http_get(url: &str, accept: Option<&str>) -> Result<Vec<u8>, String> {
    let mut req = agent().get(url);
    if let Some(a) = accept {
        req = req.header("accept", a);
    }
    let mut resp = req.call().map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    if status >= 400 {
        return Err(format!("HTTP {status} {}", status_text(status)));
    }
    resp.body_mut()
        .with_config()
        .limit(512 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| e.to_string())
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
    ids: Arc<AtomicU64>,
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
            let ids = ids.clone();
            std::thread::spawn(move || serve_connection(stream, server, tx, flag, ids));
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

fn wants_upgrade(headers: &Headers) -> bool {
    headers
        .iter()
        .any(|(k, v)| k == "upgrade" && !v.trim().is_empty())
        && headers
            .iter()
            .any(|(k, v)| k == "connection" && v.to_ascii_lowercase().contains("upgrade"))
}

fn serve_connection(
    stream: TcpStream,
    server: u64,
    tx: Sender<IoEvent>,
    stop: Arc<AtomicBool>,
    ids: Arc<AtomicU64>,
) {
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
        if wants_upgrade(&parsed.request.headers) {
            let head = reader.buffer().to_vec();
            let stream = reader.into_inner();
            let _ = stream.set_read_timeout(Some(Duration::from_millis(20)));
            let Ok(tcp) = stream.try_clone() else {
                return;
            };
            let id = ids.fetch_add(1, Ordering::SeqCst) + 1;
            let (ctx, crx) = channel::<SocketCmd>();
            if tx
                .send(IoEvent::Upgrade {
                    server,
                    id,
                    request: parsed.request,
                    local,
                    remote,
                    cmd: ctx,
                    head,
                })
                .is_err()
            {
                return;
            }
            socket_loop(tcp, stream, id, crx, tx);
            return;
        }
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
