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
    pub tls: TlsClientOptions,
    pub cancel: Arc<AtomicBool>,
    pub decode: bool,
}

pub const ACCEPT_ENCODING: &str = "gzip, deflate";

struct Inflate<R: Read> {
    inner: Option<std::io::BufReader<R>>,
    decoder: Option<Box<dyn Read + Send>>,
}

impl<R: Read + Send + 'static> Read for Inflate<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.decoder.is_none() {
            let Some(mut inner) = self.inner.take() else {
                return Ok(0);
            };
            let head = inner.fill_buf()?;
            let zlib = head.len() >= 2
                && head[0] & 0x0f == 8
                && (u16::from(head[0]) << 8 | u16::from(head[1])) % 31 == 0;
            self.decoder = Some(if zlib {
                Box::new(flate2::read::ZlibDecoder::new(inner))
            } else {
                Box::new(flate2::read::DeflateDecoder::new(inner))
            });
        }
        match self.decoder.as_mut() {
            Some(d) => d.read(buf),
            None => Ok(0),
        }
    }
}

pub fn content_decoder(
    encoding: Option<&str>,
    reader: Box<dyn Read + Send>,
) -> Box<dyn Read + Send> {
    let encoding = encoding.map(|e| e.trim().to_ascii_lowercase());
    match encoding.as_deref() {
        Some("gzip" | "x-gzip") => Box::new(flate2::read::MultiGzDecoder::new(reader)),
        Some("deflate") => Box::new(Inflate {
            inner: Some(std::io::BufReader::new(reader)),
            decoder: None,
        }),
        Some("br") => Box::new(brotli_decompressor::Decompressor::new(reader, 65536)),
        _ => reader,
    }
}

fn header_value<'a>(headers: &'a Headers, name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

pub struct FetchResponse {
    pub status: u16,
    pub status_text: String,
    pub url: String,
    pub headers: Headers,
}

pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub target: String,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub remote: String,
}

pub struct HttpReply {
    pub status: u16,
    pub headers: Headers,
    pub body: Vec<u8>,
}

pub enum ReplyMsg {
    Full(HttpReply),
    Head { status: u16, headers: Headers },
    Chunk(Vec<u8>),
    End,
    Abort,
}

pub enum IoEvent {
    Fetch {
        id: u64,
        result: Result<FetchResponse, String>,
    },
    FetchChunk {
        id: u64,
        data: Vec<u8>,
    },
    Napi(crate::napi::NapiEvent),
    FetchEnd {
        id: u64,
        error: Option<String>,
    },
    Request {
        server: u64,
        rid: u64,
        request: HttpRequest,
        reply: Sender<ReplyMsg>,
    },
    RequestClosed {
        server: u64,
        rid: u64,
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
    Watch {
        id: u64,
        event: WatchEvent,
    },
    Port {
        port: u64,
        event: crate::builtins::threads::PortEvent,
    },
    Broadcast {
        id: u64,
        data: crate::builtins::threads::Wire,
    },
    Worker {
        id: u64,
        event: crate::builtins::threads::WorkerEvent,
    },
    Wake,
    WaitDone {
        id: u64,
        result: &'static str,
    },
}

pub struct WatchEvent {
    pub kind: &'static str,
    pub filename: Option<String>,
}

pub enum WatchCmd {
    Stop,
}

type WatchSnapshot = std::collections::BTreeMap<String, (u128, u64, bool)>;

fn watch_entry(md: &std::fs::Metadata) -> (u128, u64, bool) {
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    (mtime, md.len(), md.is_dir())
}

fn watch_scan(root: &std::path::Path, is_dir: bool, recursive: bool) -> WatchSnapshot {
    let mut out = WatchSnapshot::new();
    if !is_dir {
        if let Ok(md) = std::fs::metadata(root) {
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.insert(name, watch_entry(&md));
        }
        return out;
    }
    let mut stack = vec![std::path::PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(root.join(&rel)) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(md) = entry.metadata() else {
                continue;
            };
            let rel_path = rel.join(entry.file_name());
            let entry_is_dir = md.is_dir();
            out.insert(rel_path.to_string_lossy().into_owned(), watch_entry(&md));
            if recursive && entry_is_dir {
                stack.push(rel_path);
            }
        }
    }
    out
}

pub fn watch_start(
    id: u64,
    path: std::path::PathBuf,
    recursive: bool,
    interval: Duration,
    tx: Sender<IoEvent>,
) -> std::io::Result<Sender<WatchCmd>> {
    let is_dir = std::fs::metadata(&path)?.is_dir();
    let (cmd_tx, cmd_rx) = channel::<WatchCmd>();
    std::thread::spawn(move || {
        let mut prev = watch_scan(&path, is_dir, recursive);
        loop {
            match cmd_rx.recv_timeout(interval) {
                Ok(WatchCmd::Stop) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
            let next = watch_scan(&path, is_dir, recursive);
            let mut events = Vec::new();
            for (name, entry) in &next {
                match prev.get(name) {
                    None => events.push(("rename", name.clone())),
                    Some(old) if old != entry => events.push(("change", name.clone())),
                    _ => {}
                }
            }
            for name in prev.keys() {
                if !next.contains_key(name) {
                    events.push(("rename", name.clone()));
                }
            }
            prev = next;
            for (kind, filename) in events {
                let event = WatchEvent {
                    kind,
                    filename: Some(filename),
                };
                if tx.send(IoEvent::Watch { id, event }).is_err() {
                    return;
                }
            }
        }
    });
    Ok(cmd_tx)
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
    ReadDone,
    TlsFlush,
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
    pub shell_path: Option<String>,
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

#[derive(Clone, Default)]
pub struct TlsClientOptions {
    pub servername: String,
    pub insecure: bool,
    pub ca: Option<Vec<u8>>,
}

#[derive(Debug)]
struct AcceptAnyCert(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for AcceptAnyCert {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

pub fn pem_certificates(
    pem: &[u8],
) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>, String> {
    use rustls::pki_types::pem::PemObject;
    let certs = rustls::pki_types::CertificateDer::pem_slice_iter(pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("invalid certificate: {e}"))?;
    if certs.is_empty() {
        return Err("no PEM certificate found".to_string());
    }
    Ok(certs)
}

fn client_config_for(opts: &TlsClientOptions) -> Result<Arc<rustls::ClientConfig>, String> {
    if !opts.insecure && opts.ca.is_none() {
        return Ok(tls_config());
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?;
    let config = if opts.insecure {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAnyCert(provider)))
            .with_no_client_auth()
    } else {
        let mut roots = rustls::RootCertStore::empty();
        for cert in pem_certificates(opts.ca.as_deref().unwrap_or_default())? {
            roots
                .add(cert)
                .map_err(|e| format!("invalid CA certificate: {e}"))?;
        }
        builder.with_root_certificates(roots).with_no_client_auth()
    };
    Ok(Arc::new(config))
}

pub fn tls_server_config(
    cert_pem: &[u8],
    key_pem: &[u8],
    alpn: &[&str],
) -> Result<Arc<rustls::ServerConfig>, String> {
    use rustls::pki_types::pem::PemObject;
    let certs = pem_certificates(cert_pem)?;
    let key = rustls::pki_types::PrivateKeyDer::from_pem_slice(key_pem)
        .map_err(|e| format!("invalid private key: {e}"))?;
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| e.to_string())?
    .with_no_client_auth()
    .with_single_cert(certs, key)
    .map_err(|e| e.to_string())?;
    config.alpn_protocols = alpn.iter().map(|p| p.as_bytes().to_vec()).collect();
    Ok(Arc::new(config))
}

trait CloseNotify {
    fn close_notify(&mut self) {}
}

impl CloseNotify for TcpStream {}

impl CloseNotify for rustls::StreamOwned<rustls::ServerConnection, TcpStream> {
    fn close_notify(&mut self) {
        self.conn.send_close_notify();
        let _ = self.conn.complete_io(&mut self.sock);
    }
}

fn tls_wrap(
    tcp: TcpStream,
    opts: &TlsClientOptions,
) -> Result<rustls::StreamOwned<rustls::ClientConnection, TcpStream>, String> {
    let servername = opts.servername.as_str();
    let name = rustls::pki_types::ServerName::try_from(servername.to_string())
        .map_err(|e| format!("invalid server name {servername}: {e}"))?;
    let conn =
        rustls::ClientConnection::new(client_config_for(opts)?, name).map_err(|e| e.to_string())?;
    let mut stream = rustls::StreamOwned::new(conn, tcp);
    while stream.conn.is_handshaking() {
        stream
            .conn
            .complete_io(&mut stream.sock)
            .map_err(|e| e.to_string())?;
    }
    Ok(stream)
}

pub trait IntoSock {
    fn into_sock(self) -> (TcpStream, Option<rustls::Connection>);
}

impl IntoSock for TcpStream {
    fn into_sock(self) -> (TcpStream, Option<rustls::Connection>) {
        (self, None)
    }
}

impl IntoSock for rustls::StreamOwned<rustls::ClientConnection, TcpStream> {
    fn into_sock(self) -> (TcpStream, Option<rustls::Connection>) {
        (self.sock, Some(rustls::Connection::Client(self.conn)))
    }
}

impl IntoSock for rustls::StreamOwned<rustls::ServerConnection, TcpStream> {
    fn into_sock(self) -> (TcpStream, Option<rustls::Connection>) {
        (self.sock, Some(rustls::Connection::Server(self.conn)))
    }
}

type SharedTls = Arc<std::sync::Mutex<rustls::Connection>>;

fn lock_tls(conn: &SharedTls) -> std::sync::MutexGuard<'_, rustls::Connection> {
    conn.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn drain_plaintext(conn: &mut rustls::Connection, out: &mut Vec<u8>) -> bool {
    let mut chunk = [0u8; 16384];
    loop {
        match conn.reader().read(&mut chunk) {
            Ok(0) => return true,
            Ok(n) => out.extend_from_slice(&chunk[..n]),
            Err(_) => return false,
        }
    }
}

fn tls_send(
    conn: &SharedTls,
    sock: &mut TcpStream,
    data: &[u8],
    close: bool,
) -> std::io::Result<()> {
    let mut out = Vec::with_capacity(data.len() + 64);
    {
        let mut c = lock_tls(conn);
        let mut rest = data;
        loop {
            while !rest.is_empty() {
                let n = c.writer().write(rest)?;
                if n == 0 {
                    break;
                }
                rest = &rest[n..];
            }
            while c.wants_write() {
                c.write_tls(&mut out)?;
            }
            if rest.is_empty() {
                break;
            }
        }
        if close {
            c.send_close_notify();
            while c.wants_write() {
                c.write_tls(&mut out)?;
            }
        }
    }
    if out.is_empty() {
        return Ok(());
    }
    sock.write_all(&out)
}

fn quiet_read_error(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::UnexpectedEof
    )
}

fn socket_reader(
    mut sock: TcpStream,
    conn: Option<SharedTls>,
    id: u64,
    cmd: Sender<SocketCmd>,
    closed: Arc<AtomicBool>,
    tx: Sender<IoEvent>,
) {
    let emit = |event: SocketEvent| {
        if !closed.load(Ordering::SeqCst) {
            let _ = tx.send(IoEvent::Socket { id, event });
        }
    };
    let mut failed = false;
    let mut buf = vec![0u8; 65536];
    let mut plain = Vec::new();
    if let Some(c) = &conn {
        drain_plaintext(&mut lock_tls(c), &mut plain);
        if !plain.is_empty() {
            emit(SocketEvent::Data(std::mem::take(&mut plain)));
        }
    }
    loop {
        let n = match sock.read(&mut buf) {
            Ok(n) => n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::Interrupted
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(e) => {
                if quiet_read_error(&e) {
                    emit(SocketEvent::End);
                } else {
                    let code = error_code(&e);
                    emit(SocketEvent::Error {
                        code: code.to_string(),
                        message: format!("read {code}"),
                    });
                    failed = true;
                }
                break;
            }
        };
        if n == 0 {
            emit(SocketEvent::End);
            break;
        }
        let Some(c) = &conn else {
            emit(SocketEvent::Data(buf[..n].to_vec()));
            continue;
        };
        let mut failure = None;
        let mut peer_closed = false;
        let wants_write = {
            let mut guard = lock_tls(c);
            let mut input = &buf[..n];
            while !input.is_empty() {
                if let Err(e) = guard.read_tls(&mut input) {
                    failure = Some(e.to_string());
                    break;
                }
                match guard.process_new_packets() {
                    Ok(state) => {
                        peer_closed |= drain_plaintext(&mut guard, &mut plain);
                        peer_closed |= state.peer_has_closed();
                    }
                    Err(e) => {
                        failure = Some(e.to_string());
                        break;
                    }
                }
            }
            guard.wants_write()
        };
        if !plain.is_empty() {
            emit(SocketEvent::Data(std::mem::take(&mut plain)));
        }
        if wants_write {
            let _ = cmd.send(SocketCmd::TlsFlush);
        }
        if let Some(message) = failure {
            emit(SocketEvent::Error {
                code: "ERR_SSL_PROTOCOL_ERROR".to_string(),
                message,
            });
            failed = true;
            break;
        }
        if peer_closed {
            emit(SocketEvent::End);
            break;
        }
    }
    let _ = cmd.send(if failed {
        SocketCmd::Destroy
    } else {
        SocketCmd::ReadDone
    });
}

fn socket_loop<S: IntoSock>(
    stream: S,
    id: u64,
    cmd: Sender<SocketCmd>,
    rx: Receiver<SocketCmd>,
    tx: Sender<IoEvent>,
) {
    let (mut sock, conn) = stream.into_sock();
    let _ = sock.set_read_timeout(None);
    let conn = conn.map(|mut c| {
        c.set_buffer_limit(None);
        Arc::new(std::sync::Mutex::new(c))
    });
    let closed = Arc::new(AtomicBool::new(false));
    match sock.try_clone() {
        Ok(read_sock) => {
            let conn = conn.clone();
            let closed = closed.clone();
            let tx = tx.clone();
            std::thread::spawn(move || socket_reader(read_sock, conn, id, cmd, closed, tx));
        }
        Err(e) => {
            let code = error_code(&e);
            let _ = tx.send(IoEvent::Socket {
                id,
                event: SocketEvent::Error {
                    code: code.to_string(),
                    message: format!("read {code}"),
                },
            });
            let _ = cmd.send(SocketCmd::Destroy);
        }
    }
    let mut read_done = false;
    let mut write_done = false;
    while let Ok(command) = rx.recv() {
        match command {
            SocketCmd::Write(data) => {
                if write_done {
                    continue;
                }
                let r = match &conn {
                    None => sock.write_all(&data),
                    Some(c) => tls_send(c, &mut sock, &data, false),
                };
                if let Err(e) = r {
                    let code = error_code(&e);
                    if !closed.load(Ordering::SeqCst) {
                        let _ = tx.send(IoEvent::Socket {
                            id,
                            event: SocketEvent::Error {
                                code: code.to_string(),
                                message: format!("write {code}"),
                            },
                        });
                    }
                    break;
                }
            }
            SocketCmd::TlsFlush => {
                if let Some(c) = &conn {
                    let _ = tls_send(c, &mut sock, &[], false);
                }
            }
            SocketCmd::End => {
                if !write_done {
                    if let Some(c) = &conn {
                        let _ = tls_send(c, &mut sock, &[], true);
                    }
                    let _ = sock.flush();
                    let _ = sock.shutdown(std::net::Shutdown::Write);
                    write_done = true;
                }
                if read_done {
                    break;
                }
            }
            SocketCmd::ReadDone => {
                read_done = true;
                if write_done {
                    break;
                }
            }
            SocketCmd::Flush(ack) => {
                let _ = sock.flush();
                let _ = ack.send(());
            }
            SocketCmd::Destroy => break,
        }
    }
    closed.store(true, Ordering::SeqCst);
    let _ = sock.shutdown(std::net::Shutdown::Both);
    let _ = tx.send(IoEvent::Socket {
        id,
        event: SocketEvent::Closed,
    });
}

pub fn socket_connect(
    id: u64,
    host: String,
    port: u16,
    tls: Option<TlsClientOptions>,
    tx: Sender<IoEvent>,
) -> Sender<SocketCmd> {
    let (ctx, crx) = channel::<SocketCmd>();
    let own = ctx.clone();
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
        let stream = match connect_any(&addrs, Duration::from_secs(60)) {
            Ok(s) => s,
            Err(e) => {
                let code = error_code(&e);
                fail(code, format!("connect {code} {host}:{port}"));
                return;
            }
        };
        let _ = stream.set_nodelay(true);
        let (Ok(local), Ok(remote)) = (stream.local_addr(), stream.peer_addr()) else {
            fail("EIO", "connect failed".to_string());
            return;
        };
        match tls {
            None => {
                let _ = tx.send(IoEvent::Socket {
                    id,
                    event: SocketEvent::Connected { local, remote },
                });
                socket_loop(stream, id, own, crx, tx);
            }
            Some(opts) => match tls_wrap(stream, &opts) {
                Ok(secure) => {
                    let _ = tx.send(IoEvent::Socket {
                        id,
                        event: SocketEvent::Connected { local, remote },
                    });
                    socket_loop(secure, id, own, crx, tx);
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
    tls: Option<Arc<rustls::ServerConfig>>,
) -> std::io::Result<ServerHandle> {
    let listener = bind_listener(host, port)?;
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
            let _ = stream.set_nodelay(true);
            let id = ids.fetch_add(1, Ordering::SeqCst) + 1;
            let (ctx, crx) = channel::<SocketCmd>();
            let own = ctx.clone();
            let tx2 = tx.clone();
            match tls.clone() {
                None => {
                    let _ = tx.send(IoEvent::Accept {
                        server,
                        id,
                        local,
                        remote,
                        cmd: ctx,
                    });
                    std::thread::spawn(move || socket_loop(stream, id, own, crx, tx2));
                }
                Some(config) => {
                    std::thread::spawn(move || {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                        let Ok(conn) = rustls::ServerConnection::new(config) else {
                            return;
                        };
                        let mut secure = rustls::StreamOwned::new(conn, stream);
                        while secure.conn.is_handshaking() {
                            if secure.conn.complete_io(&mut secure.sock).is_err() {
                                let _ = secure.sock.shutdown(std::net::Shutdown::Both);
                                return;
                            }
                        }
                        if tx2
                            .send(IoEvent::Accept {
                                server,
                                id,
                                local,
                                remote,
                                cmd: ctx,
                            })
                            .is_err()
                        {
                            return;
                        }
                        socket_loop(secure, id, own, crx, tx2);
                    });
                }
            }
        }
    });
    Ok(ServerHandle { addr, stop })
}

#[cfg(windows)]
fn shell_for(line: &str, shell: Option<&str>) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let program = shell.unwrap_or("cmd");
    let base = std::path::Path::new(program)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(program)
        .to_ascii_lowercase();
    let mut c = std::process::Command::new(program);
    if base == "cmd" {
        c.args(["/d", "/s", "/c"]);
        c.raw_arg(format!("\"{line}\""));
    } else {
        c.arg("-c").arg(line);
    }
    c
}

#[cfg(not(windows))]
fn shell_for(line: &str, shell: Option<&str>) -> std::process::Command {
    let mut c = std::process::Command::new(shell.unwrap_or("sh"));
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
        shell_for(&line, spec.shell_path.as_deref())
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

#[derive(Debug, Default)]
struct LoopbackResolver {
    inner: ureq::unversioned::resolver::DefaultResolver,
}

impl ureq::unversioned::resolver::Resolver for LoopbackResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        let mut addrs = self.inner.resolve(uri, config, timeout)?;
        if addrs.len() > 1 && addrs.iter().all(|a| a.ip().is_loopback()) {
            let list: Vec<SocketAddr> = addrs.iter().copied().collect();
            if let Ok(stream) = connect_any(&list, Duration::from_secs(2)) {
                if let Ok(peer) = stream.peer_addr() {
                    if let Some(i) = addrs.iter().position(|a| *a == peer) {
                        addrs.swap(0, i);
                    }
                }
            }
        }
        Ok(addrs)
    }
}

#[derive(Debug)]
struct FastTcpConnector;

struct FastTcp {
    stream: TcpStream,
    buffers: ureq::unversioned::transport::LazyBuffers,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}

impl std::fmt::Debug for FastTcp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FastTcp").finish()
    }
}

fn transport_error(e: std::io::Error, reason: ureq::Timeout) -> ureq::Error {
    match e.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
            ureq::Error::Timeout(reason)
        }
        _ => e.into(),
    }
}

impl<In: ureq::unversioned::transport::Transport> ureq::unversioned::transport::Connector<In>
    for FastTcpConnector
{
    type Out = ureq::unversioned::transport::Either<In, FastTcp>;

    fn connect(
        &self,
        details: &ureq::unversioned::transport::ConnectionDetails,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        use ureq::unversioned::transport::Either;
        if let Some(t) = chained {
            return Ok(Some(Either::A(t)));
        }
        let addrs: Vec<SocketAddr> = details.addrs.iter().copied().collect();
        let timeout = details
            .timeout
            .not_zero()
            .map(|d| *d)
            .unwrap_or(Duration::from_secs(60));
        let stream =
            connect_any(&addrs, timeout).map_err(|e| transport_error(e, ureq::Timeout::Connect))?;
        if details.config.no_delay() {
            stream.set_nodelay(true)?;
        }
        let buffers = ureq::unversioned::transport::LazyBuffers::new(
            details.config.input_buffer_size(),
            details.config.output_buffer_size(),
        );
        Ok(Some(Either::B(FastTcp {
            stream,
            buffers,
            read_timeout: None,
            write_timeout: None,
        })))
    }
}

impl ureq::unversioned::transport::Transport for FastTcp {
    fn buffers(&mut self) -> &mut dyn ureq::unversioned::transport::Buffers {
        &mut self.buffers
    }

    fn transmit_output(
        &mut self,
        amount: usize,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<(), ureq::Error> {
        use ureq::unversioned::transport::Buffers;
        let wanted = timeout.not_zero().map(|d| *d);
        if wanted != self.write_timeout {
            self.stream.set_write_timeout(wanted)?;
            self.write_timeout = wanted;
        }
        let output = &self.buffers.output()[..amount];
        self.stream
            .write_all(output)
            .map_err(|e| transport_error(e, timeout.reason))
    }

    fn await_input(
        &mut self,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<bool, ureq::Error> {
        use ureq::unversioned::transport::Buffers;
        let wanted = timeout.not_zero().map(|d| *d);
        if wanted != self.read_timeout {
            self.stream.set_read_timeout(wanted)?;
            self.read_timeout = wanted;
        }
        let input = self.buffers.input_append_buf();
        let amount = self
            .stream
            .read(input)
            .map_err(|e| transport_error(e, timeout.reason))?;
        self.buffers.input_appended(amount);
        Ok(amount > 0)
    }

    fn is_open(&mut self) -> bool {
        if self.stream.set_nonblocking(true).is_err() {
            return false;
        }
        let mut buf = [0u8; 1];
        let open = matches!(
            self.stream.read(&mut buf),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
        );
        open && self.stream.set_nonblocking(false).is_ok()
    }
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        use ureq::unversioned::transport::Connector;
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .user_agent(format!("javasquirt/{}", crate::VERSION))
            .build();
        let connector = ()
            .chain(ureq::unversioned::transport::ConnectProxyConnector::default())
            .chain(FastTcpConnector)
            .chain(ureq::unversioned::transport::RustlsConnector::default());
        ureq::Agent::with_parts(config, connector, LoopbackResolver::default())
    })
}

fn interleave_families(addrs: &[SocketAddr]) -> Vec<SocketAddr> {
    let Some(first) = addrs.first() else {
        return Vec::new();
    };
    let (mut same, mut other): (std::collections::VecDeque<_>, std::collections::VecDeque<_>) =
        addrs
            .iter()
            .copied()
            .partition(|a| a.is_ipv4() == first.is_ipv4());
    let mut out = Vec::with_capacity(addrs.len());
    loop {
        match (same.pop_front(), other.pop_front()) {
            (None, None) => break,
            (a, b) => {
                out.extend(a);
                out.extend(b);
            }
        }
    }
    out
}

#[cfg(windows)]
fn fail_fast_on_loopback(socket: &socket2::Socket) {
    use std::os::windows::io::AsRawSocket;
    #[repr(C)]
    struct RetransmitParams {
        rtt: u16,
        max_syn_retransmissions: u8,
    }
    #[link(name = "ws2_32")]
    unsafe extern "system" {
        fn WSAIoctl(
            socket: usize,
            code: u32,
            input: *const std::ffi::c_void,
            input_len: u32,
            output: *mut std::ffi::c_void,
            output_len: u32,
            returned: *mut u32,
            overlapped: *mut std::ffi::c_void,
            completion: *mut std::ffi::c_void,
        ) -> i32;
    }
    const RETRANSMIT_IOCTL: u32 = 0x9800_0011;
    const NO_SYN_RETRANSMISSIONS: u8 = 0xFE;
    let params = RetransmitParams {
        rtt: NO_SYN_RETRANSMISSIONS as u16,
        max_syn_retransmissions: NO_SYN_RETRANSMISSIONS,
    };
    let mut returned = 0u32;
    unsafe {
        WSAIoctl(
            socket.as_raw_socket() as usize,
            RETRANSMIT_IOCTL,
            &params as *const RetransmitParams as *const std::ffi::c_void,
            std::mem::size_of::<RetransmitParams>() as u32,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
}

pub fn tcp_connect(addr: &SocketAddr, timeout: Duration) -> std::io::Result<TcpStream> {
    #[cfg(windows)]
    if addr.ip().is_loopback() {
        let socket = socket2::Socket::new(
            socket2::Domain::for_address(*addr),
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )?;
        fail_fast_on_loopback(&socket);
        socket.connect_timeout(&(*addr).into(), timeout)?;
        return Ok(socket.into());
    }
    TcpStream::connect_timeout(addr, timeout)
}

pub fn connect_any(addrs: &[SocketAddr], timeout: Duration) -> std::io::Result<TcpStream> {
    if addrs.len() == 1 {
        return tcp_connect(&addrs[0], timeout);
    }
    let order = interleave_families(addrs);
    let (rtx, rrx) = channel::<std::io::Result<TcpStream>>();
    let mut started = 0;
    let mut finished = 0;
    let mut last = None;
    loop {
        if started < order.len() {
            let addr = order[started];
            let rtx = rtx.clone();
            std::thread::spawn(move || {
                let _ = rtx.send(tcp_connect(&addr, timeout));
            });
            started += 1;
        }
        let wait = if started < order.len() {
            Duration::from_millis(250)
        } else {
            timeout + Duration::from_secs(1)
        };
        match rrx.recv_timeout(wait) {
            Ok(Ok(stream)) => return Ok(stream),
            Ok(Err(e)) => {
                finished += 1;
                last = Some(e);
                if finished == order.len() {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if started == order.len() {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::from(std::io::ErrorKind::TimedOut)))
}

fn bind_dual_stack(port: u16) -> std::io::Result<TcpListener> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV6,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )?;
    socket.set_only_v6(false)?;
    #[cfg(not(windows))]
    socket.set_reuse_address(true)?;
    let addr = SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, port));
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    Ok(socket.into())
}

pub fn bind_listener(host: &str, port: u16) -> std::io::Result<TcpListener> {
    if host == "::" {
        return bind_dual_stack(port).or_else(|_| TcpListener::bind(("0.0.0.0", port)));
    }
    TcpListener::bind((host, port))
}

pub fn http_get(url: &str, accept: Option<&str>) -> Result<Vec<u8>, String> {
    let mut req = agent().get(url).header("accept-encoding", ACCEPT_ENCODING);
    if let Some(a) = accept {
        req = req.header("accept", a);
    }
    let resp = req.call().map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    if status >= 400 {
        return Err(format!("HTTP {status} {}", status_text(status)));
    }
    let encoding = resp
        .headers()
        .get("content-encoding")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = resp.into_body().into_with_config().limit(u64::MAX).reader();
    let mut reader = content_decoder(encoding.as_deref(), Box::new(body)).take(512 * 1024 * 1024);
    let mut out = Vec::new();
    reader.read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
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

fn fetch_tls_config(opts: &TlsClientOptions) -> Result<ureq::tls::TlsConfig, String> {
    let mut builder = ureq::tls::TlsConfig::builder();
    if opts.insecure {
        builder = builder.disable_verification(true);
    } else if let Some(ca) = &opts.ca {
        let certs = pem_certificates(ca)?
            .into_iter()
            .map(|c| ureq::tls::Certificate::from_der(c.as_ref()).to_owned())
            .collect::<Vec<_>>();
        builder = builder.root_certs(ureq::tls::RootCerts::Specific(Arc::new(certs)));
    }
    Ok(builder.build())
}

fn run_fetch(req: FetchRequest) -> Result<(FetchResponse, ureq::Body), String> {
    use ureq::RequestExt;
    let mut builder = ureq::http::Request::builder()
        .method(req.method.as_str())
        .uri(req.url.as_str());
    for (k, v) in &req.headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    if req.decode && header_value(&req.headers, "accept-encoding").is_none() {
        builder = builder.header("accept-encoding", ACCEPT_ENCODING);
    }
    let redirects = if req.follow { 20 } else { 0 };
    let tls = fetch_tls_config(&req.tls)?;
    let resp = match req.body {
        Some(body) => builder
            .body(body)
            .map_err(|e| e.to_string())?
            .with_agent(agent())
            .configure()
            .max_redirects(redirects)
            .max_redirects_will_error(false)
            .timeout_global(req.timeout)
            .allow_non_standard_methods(true)
            .tls_config(tls)
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
            .tls_config(tls)
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
    let (_, body) = resp.into_parts();
    Ok((
        FetchResponse {
            status,
            status_text: status_text(status).to_string(),
            url,
            headers,
        },
        body,
    ))
}

pub fn spawn_fetch(tx: Sender<IoEvent>, id: u64, req: FetchRequest) {
    std::thread::spawn(move || {
        let cancel = req.cancel.clone();
        let decode = req.decode;
        let mut encoding = None;
        let body = match run_fetch(req) {
            Ok((head, body)) => {
                if decode {
                    encoding = header_value(&head.headers, "content-encoding").map(str::to_string);
                }
                if tx
                    .send(IoEvent::Fetch {
                        id,
                        result: Ok(head),
                    })
                    .is_err()
                {
                    return;
                }
                body
            }
            Err(e) => {
                let _ = tx.send(IoEvent::Fetch { id, result: Err(e) });
                return;
            }
        };
        let raw = body.into_with_config().limit(u64::MAX).reader();
        let mut reader = content_decoder(encoding.as_deref(), Box::new(raw));
        let mut buf = vec![0u8; 65536];
        let error = loop {
            if cancel.load(Ordering::SeqCst) {
                return;
            }
            match reader.read(&mut buf) {
                Ok(0) => break None,
                Ok(n) => {
                    if tx
                        .send(IoEvent::FetchChunk {
                            id,
                            data: buf[..n].to_vec(),
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => break Some(e.to_string()),
            }
        };
        let _ = tx.send(IoEvent::FetchEnd { id, error });
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
    tls: Option<Arc<rustls::ServerConfig>>,
) -> std::io::Result<ServerHandle> {
    let listener = bind_listener(host, port)?;
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
            let tls = tls.clone();
            std::thread::spawn(move || serve_connection(stream, server, tx, flag, ids, tls));
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
    http10: bool,
}

fn read_request(
    r: &mut impl BufRead,
    local: &SocketAddr,
    remote: &SocketAddr,
    secure: bool,
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
        target.clone()
    } else if secure {
        format!("https://{host}{target}")
    } else {
        format!("http://{host}{target}")
    };
    let head = method == "HEAD";
    let http10 = version == "HTTP/1.0";
    Some(Parsed {
        http10,
        request: HttpRequest {
            method,
            url,
            target,
            headers,
            body,
            remote: remote.ip().to_string(),
        },
        keep_alive,
        head,
    })
}

fn http_date() -> String {
    thread_local! {
        static CACHE: std::cell::RefCell<(i64, String)> = const { std::cell::RefCell::new((i64::MIN, String::new())) };
    }
    let now = chrono::Utc::now();
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.0 != now.timestamp() {
            *c = (
                now.timestamp(),
                now.format("%a, %d %b %Y %H:%M:%S GMT").to_string(),
            );
        }
        c.1.clone()
    })
}

fn write_reply(
    w: &mut impl Write,
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

fn write_head(
    w: &mut impl Write,
    status: u16,
    headers: &Headers,
    keep_alive: bool,
    chunked: bool,
) -> std::io::Result<()> {
    let mut out = Vec::with_capacity(256);
    let _ = write!(out, "HTTP/1.1 {} {}\r\n", status, status_text(status));
    let mut has_date = false;
    for (k, v) in headers {
        let lower = k.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "transfer-encoding" | "connection" | "keep-alive"
        ) {
            continue;
        }
        has_date |= lower == "date";
        let _ = write!(out, "{k}: {v}\r\n");
    }
    if !has_date {
        let _ = write!(out, "date: {}\r\n", http_date());
    }
    if chunked {
        out.extend_from_slice(b"transfer-encoding: chunked\r\n");
    }
    if keep_alive {
        out.extend_from_slice(b"connection: keep-alive\r\nkeep-alive: timeout=5\r\n\r\n");
    } else {
        out.extend_from_slice(b"connection: close\r\n\r\n");
    }
    w.write_all(&out)?;
    w.flush()
}

fn write_chunk(w: &mut impl Write, data: &[u8]) -> std::io::Result<()> {
    let mut out = Vec::with_capacity(data.len() + 16);
    let _ = write!(out, "{:x}\r\n", data.len());
    out.extend_from_slice(data);
    out.extend_from_slice(b"\r\n");
    w.write_all(&out)?;
    w.flush()
}

fn peer_gone(tcp: &TcpStream) -> bool {
    if tcp.set_nonblocking(true).is_err() {
        return false;
    }
    let mut probe = [0u8; 1];
    let r = tcp.peek(&mut probe);
    let _ = tcp.set_nonblocking(false);
    match r {
        Ok(0) => true,
        Ok(_) => false,
        Err(e) => e.kind() != std::io::ErrorKind::WouldBlock,
    }
}

enum Waited {
    Msg(ReplyMsg),
    Dropped,
}

fn wait_reply(
    rx: &Receiver<ReplyMsg>,
    tcp: &TcpStream,
    notified: &mut bool,
    notify: &dyn Fn(),
) -> Waited {
    loop {
        if *notified {
            return match rx.recv() {
                Ok(m) => Waited::Msg(m),
                Err(_) => Waited::Dropped,
            };
        }
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(m) => return Waited::Msg(m),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Waited::Dropped,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if peer_gone(tcp) {
                    *notified = true;
                    notify();
                }
            }
        }
    }
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
    tls: Option<Arc<rustls::ServerConfig>>,
) {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let (Ok(local), Ok(remote)) = (stream.local_addr(), stream.peer_addr()) else {
        return;
    };
    let Ok(tcp) = stream.try_clone() else {
        return;
    };
    match tls {
        None => serve_stream(tcp, stream, local, remote, server, tx, stop, ids, false),
        Some(config) => {
            let Ok(conn) = rustls::ServerConnection::new(config) else {
                return;
            };
            let secure = rustls::StreamOwned::new(conn, stream);
            serve_stream(tcp, secure, local, remote, server, tx, stop, ids, true);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn serve_stream<S: Read + Write + CloseNotify + IntoSock + Send + 'static>(
    tcp: TcpStream,
    stream: S,
    local: SocketAddr,
    remote: SocketAddr,
    server: u64,
    tx: Sender<IoEvent>,
    stop: Arc<AtomicBool>,
    ids: Arc<AtomicU64>,
    secure: bool,
) {
    let mut reader = BufReader::new(stream);
    while !stop.load(Ordering::SeqCst) {
        let Some(parsed) = read_request(&mut reader, &local, &remote, secure) else {
            let _ = tcp.shutdown(std::net::Shutdown::Both);
            return;
        };
        if parsed.request.method == "CONNECT" || wants_upgrade(&parsed.request.headers) {
            let head = reader.buffer().to_vec();
            let stream = reader.into_inner();
            let id = ids.fetch_add(1, Ordering::SeqCst) + 1;
            let (ctx, crx) = channel::<SocketCmd>();
            let own = ctx.clone();
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
            socket_loop(stream, id, own, crx, tx);
            return;
        }
        let rid = ids.fetch_add(1, Ordering::SeqCst) + 1;
        let (reply_tx, reply_rx) = channel();
        if tx
            .send(IoEvent::Request {
                server,
                rid,
                request: parsed.request,
                reply: reply_tx,
            })
            .is_err()
        {
            return;
        }
        let mut notified = false;
        let notify = || {
            let _ = tx.send(IoEvent::RequestClosed { server, rid });
        };
        let close = |writer: &mut S| {
            let _ = writer.flush();
            writer.close_notify();
            let _ = tcp.shutdown(std::net::Shutdown::Both);
        };
        let Waited::Msg(first) = wait_reply(&reply_rx, &tcp, &mut notified, &notify) else {
            let _ = tcp.shutdown(std::net::Shutdown::Both);
            return;
        };
        let keep_alive = parsed.keep_alive && !stop.load(Ordering::SeqCst);
        let writer = reader.get_mut();
        match first {
            ReplyMsg::Full(reply) => {
                if write_reply(writer, &reply, keep_alive, parsed.head).is_err() || !keep_alive {
                    close(writer);
                    return;
                }
            }
            ReplyMsg::Head { status, headers } => {
                let no_body =
                    parsed.head || status == 204 || status == 304 || (100..200).contains(&status);
                let has_len = headers
                    .iter()
                    .any(|(k, _)| k.eq_ignore_ascii_case("content-length"));
                let chunked = !no_body && !has_len && !parsed.http10;
                let keep_alive = keep_alive && (no_body || has_len || chunked);
                let mut ok = write_head(writer, status, &headers, keep_alive, chunked).is_ok();
                while ok {
                    match wait_reply(&reply_rx, &tcp, &mut notified, &notify) {
                        Waited::Msg(ReplyMsg::Chunk(data)) => {
                            if no_body || data.is_empty() {
                                continue;
                            }
                            let r = if chunked {
                                write_chunk(writer, &data)
                            } else {
                                writer.write_all(&data).and_then(|_| writer.flush())
                            };
                            ok = r.is_ok();
                        }
                        Waited::Msg(ReplyMsg::End) => {
                            if chunked {
                                ok = writer
                                    .write_all(b"0\r\n\r\n")
                                    .and_then(|_| writer.flush())
                                    .is_ok();
                            }
                            break;
                        }
                        Waited::Msg(ReplyMsg::Full(_) | ReplyMsg::Head { .. }) => {}
                        Waited::Msg(ReplyMsg::Abort) | Waited::Dropped => ok = false,
                    }
                }
                if !ok {
                    if !notified {
                        notify();
                    }
                    close(writer);
                    return;
                }
                if !keep_alive {
                    close(writer);
                    return;
                }
            }
            ReplyMsg::Chunk(_) | ReplyMsg::End | ReplyMsg::Abort => {
                close(writer);
                return;
            }
        }
    }
}
