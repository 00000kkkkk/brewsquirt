use super::arg;
use crate::io::{
    self, ChildCmd, ChildEvent, ChildSpec, ServerHandle, SocketCmd, SocketEvent, StdioMode,
};
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use std::net::SocketAddr;
use std::sync::mpsc::Sender;

type R = Result<Value, Value>;

pub struct SocketEntry {
    pub cmd: Sender<SocketCmd>,
    pub handler: Value,
    pub refed: bool,
}

pub struct NetServerEntry {
    pub handler: Value,
    pub handle: ServerHandle,
    pub refed: bool,
}

pub struct ChildEntry {
    pub cmd: Sender<ChildCmd>,
    pub handler: Value,
    pub refed: bool,
}

pub struct WatchEntry {
    pub cmd: Sender<io::WatchCmd>,
    pub handler: Value,
    pub refed: bool,
}

pub fn watch_start(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let path = vm.to_string(&arg(args, 0))?.to_string();
    let recursive = arg(args, 1).truthy();
    let interval = match arg(args, 2) {
        Value::Number(n) if n.is_finite() && n >= 1.0 => n,
        _ => 200.0,
    };
    let handler = arg(args, 3);
    let id = vm.io.next_id();
    let cmd = io::watch_start(
        id,
        std::path::PathBuf::from(&path),
        recursive,
        std::time::Duration::from_millis(interval as u64),
        vm.io.tx.clone(),
    )
    .map_err(|e| super::node::fs_error(vm, &e, "watch", &path, None))?;
    vm.io.pending += 1;
    vm.io_watchers.insert(
        id,
        WatchEntry {
            cmd,
            handler,
            refed: true,
        },
    );
    Ok(Value::Number(id as f64))
}

pub fn watch_close(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args)?;
    if let Some(e) = vm.io_watchers.remove(&id) {
        let _ = e.cmd.send(io::WatchCmd::Stop);
        if e.refed {
            adjust_pending(vm, -1);
        }
    }
    Ok(Value::Undefined)
}

pub fn watch_ref(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args)?;
    let want = arg(args, 1).truthy();
    let mut delta = 0i64;
    if let Some(e) = vm.io_watchers.get_mut(&id) {
        if e.refed != want {
            e.refed = want;
            delta = if want { 1 } else { -1 };
        }
    }
    adjust_pending(vm, delta);
    Ok(Value::Undefined)
}

pub fn handle_watch(vm: &mut Vm, id: u64, event: io::WatchEvent) -> Result<(), Value> {
    let Some(handler) = vm.io_watchers.get(&id).map(|e| e.handler.clone()) else {
        return Ok(());
    };
    let filename = match event.filename {
        Some(f) => Value::from(f),
        None => Value::Null,
    };
    vm.call_value(&handler, Value::Undefined, &[event.kind.into(), filename])?;
    Ok(())
}

fn id_arg(vm: &mut Vm, args: &[Value]) -> Result<u64, Value> {
    Ok(vm.to_number(&arg(args, 0))? as u64)
}

fn bytes_arg(vm: &mut Vm, v: &Value) -> Result<Vec<u8>, Value> {
    match super::binary::buffer_source_bytes(v) {
        Some(b) => Ok(b),
        None => Ok(vm.to_string(v)?.as_bytes().to_vec()),
    }
}

fn port_arg(vm: &mut Vm, v: &Value) -> Result<u16, Value> {
    let port = vm.to_number(v)?;
    if !(0.0..=65535.0).contains(&port) || port.fract() != 0.0 {
        return Err(vm.range_error(format!(
            "port should be >= 0 and < 65536. Received {}",
            crate::number::number_to_string(port)
        )));
    }
    Ok(port as u16)
}

pub fn net_connect(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let host = vm.to_string(&arg(args, 0))?.to_string();
    let port = port_arg(vm, &arg(args, 1))?;
    let tls = arg(args, 2).truthy();
    let servername = match arg(args, 3) {
        Value::Undefined | Value::Null => host.clone(),
        v => vm.to_string(&v)?.to_string(),
    };
    let handler = arg(args, 4);
    let id = vm.io.next_id();
    let cmd = io::socket_connect(id, host, port, tls.then_some(servername), vm.io.tx.clone());
    vm.io.pending += 1;
    vm.io_sockets.insert(
        id,
        SocketEntry {
            cmd,
            handler,
            refed: true,
        },
    );
    Ok(Value::Number(id as f64))
}

pub fn net_attach(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args)?;
    if let Some(e) = vm.io_sockets.get_mut(&id) {
        e.handler = arg(args, 1);
    }
    Ok(Value::Undefined)
}

pub fn net_write(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args)?;
    let data = bytes_arg(vm, &arg(args, 1))?;
    if let Some(e) = vm.io_sockets.get(&id) {
        let _ = e.cmd.send(SocketCmd::Write(data));
    }
    Ok(Value::Undefined)
}

pub fn net_end(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args)?;
    if let Some(e) = vm.io_sockets.get(&id) {
        let _ = e.cmd.send(SocketCmd::End);
    }
    Ok(Value::Undefined)
}

pub fn net_destroy(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args)?;
    if let Some(e) = vm.io_sockets.get(&id) {
        let _ = e.cmd.send(SocketCmd::Destroy);
    }
    Ok(Value::Undefined)
}

pub fn net_ref(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args)?;
    let want = arg(args, 1).truthy();
    let mut delta = 0i64;
    if let Some(e) = vm.io_sockets.get_mut(&id) {
        if e.refed != want {
            e.refed = want;
            delta = if want { 1 } else { -1 };
        }
    }
    adjust_pending(vm, delta);
    Ok(Value::Undefined)
}

fn adjust_pending(vm: &mut Vm, delta: i64) {
    if delta > 0 {
        vm.io.pending += delta as usize;
    } else if delta < 0 {
        vm.io.pending = vm.io.pending.saturating_sub((-delta) as usize);
    }
}

pub fn net_listen(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let host = vm.to_string(&arg(args, 0))?.to_string();
    let port = port_arg(vm, &arg(args, 1))?;
    let handler = arg(args, 2);
    let id = vm.io.next_id();
    let handle = match io::socket_listen(&host, port, id, vm.io.ids.clone(), vm.io.tx.clone()) {
        Ok(h) => h,
        Err(e) => {
            let code = io::error_code(&e);
            let err = vm.make_error(
                ErrorKind::Error,
                format!("listen {code}: address {host}:{port} ({e})"),
            );
            if let Value::Object(o) = &err {
                o.set_own("code", code.into());
                o.set_own("syscall", "listen".into());
                o.set_own("address", host.clone().into());
                o.set_own("port", Value::Number(port as f64));
            }
            return Err(err);
        }
    };
    let addr = handle.addr;
    vm.io.pending += 1;
    vm.io_net_servers.insert(
        id,
        NetServerEntry {
            handler,
            handle,
            refed: true,
        },
    );
    Ok(vm.new_array(vec![
        Value::Number(id as f64),
        Value::Number(addr.port() as f64),
        addr.ip().to_string().into(),
    ]))
}

pub fn net_server_close(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args)?;
    if let Some(e) = vm.io_net_servers.remove(&id) {
        e.handle.close();
        if e.refed {
            adjust_pending(vm, -1);
        }
    }
    Ok(Value::Undefined)
}

pub fn net_server_ref(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args)?;
    let want = arg(args, 1).truthy();
    let mut delta = 0i64;
    if let Some(e) = vm.io_net_servers.get_mut(&id) {
        if e.refed != want {
            e.refed = want;
            delta = if want { 1 } else { -1 };
        }
    }
    adjust_pending(vm, delta);
    Ok(Value::Undefined)
}

fn stdio_mode(vm: &mut Vm, v: &Value) -> Result<StdioMode, Value> {
    Ok(match v {
        Value::Undefined | Value::Null => StdioMode::Pipe,
        v => match &*vm.to_string(v)? {
            "inherit" => StdioMode::Inherit,
            "ignore" => StdioMode::Null,
            _ => StdioMode::Pipe,
        },
    })
}

pub fn child_spawn(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let program = vm.to_string(&arg(args, 0))?.to_string();
    let list = arg(args, 1);
    let mut argv = Vec::new();
    if list.is_object() {
        for v in vm.iterate(&list)? {
            argv.push(vm.to_string(&v)?.to_string());
        }
    }
    let options = arg(args, 2);
    let handler = arg(args, 3);
    let mut spec = ChildSpec {
        program,
        args: argv,
        cwd: None,
        env: None,
        shell: false,
        shell_path: None,
        stdin: StdioMode::Pipe,
        stdout: StdioMode::Pipe,
        stderr: StdioMode::Pipe,
    };
    if options.is_object() {
        if let Value::Str(dir) = vm.get_prop(&options, "cwd")? {
            spec.cwd = Some(dir.to_string());
        }
        let shell = vm.get_prop(&options, "shell")?;
        spec.shell = shell.truthy();
        if let Value::Str(path) = &shell {
            spec.shell_path = Some(path.to_string());
        }
        let env = vm.get_prop(&options, "env")?;
        if env.is_object() {
            let mut pairs = Vec::new();
            for (k, v) in vm.own_entries(&env) {
                if matches!(v, Value::Undefined) {
                    continue;
                }
                let v = vm.to_string(&v)?;
                pairs.push((k.to_string(), v.to_string()));
            }
            spec.env = Some(pairs);
        }
        let stdio = vm.get_prop(&options, "stdio")?;
        if stdio.is_object() {
            let modes = vm.iterate(&stdio)?;
            spec.stdin = stdio_mode(vm, modes.first().unwrap_or(&Value::Undefined))?;
            spec.stdout = stdio_mode(vm, modes.get(1).unwrap_or(&Value::Undefined))?;
            spec.stderr = stdio_mode(vm, modes.get(2).unwrap_or(&Value::Undefined))?;
        } else if !stdio.is_undefined() {
            let mode = stdio_mode(vm, &stdio)?;
            spec.stdin = mode;
            spec.stdout = mode;
            spec.stderr = mode;
        }
    }
    let id = vm.io.next_id();
    let program = spec.program.clone();
    match io::child_spawn(id, spec, vm.io.tx.clone()) {
        Ok((cmd, pid)) => {
            vm.io.pending += 1;
            vm.io_children.insert(
                id,
                ChildEntry {
                    cmd,
                    handler,
                    refed: true,
                },
            );
            Ok(vm.new_array(vec![Value::Number(id as f64), Value::Number(pid as f64)]))
        }
        Err(e) => {
            let code = match e.kind() {
                std::io::ErrorKind::NotFound => "ENOENT",
                std::io::ErrorKind::PermissionDenied => "EACCES",
                _ => "EIO",
            };
            let err = vm.make_error(ErrorKind::Error, format!("spawn {program} {code}"));
            if let Value::Object(o) = &err {
                o.set_own("code", code.into());
                o.set_own(
                    "errno",
                    Value::Number(if code == "ENOENT" { -2.0 } else { -13.0 }),
                );
                o.set_own("syscall", format!("spawn {program}").into());
                o.set_own("path", program.into());
            }
            Err(err)
        }
    }
}

pub fn child_write(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args)?;
    let data = bytes_arg(vm, &arg(args, 1))?;
    if let Some(e) = vm.io_children.get(&id) {
        let _ = e.cmd.send(ChildCmd::Write(data));
    }
    Ok(Value::Undefined)
}

pub fn child_end_stdin(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args)?;
    if let Some(e) = vm.io_children.get(&id) {
        let _ = e.cmd.send(ChildCmd::EndStdin);
    }
    Ok(Value::Undefined)
}

pub fn child_kill(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args)?;
    let signal = match arg(args, 1) {
        Value::Undefined | Value::Null => 15,
        Value::Number(n) => n as i32,
        v => {
            let name = vm.to_string(&v)?;
            match io::signal_number(&name) {
                Some(n) => n,
                None => {
                    let err = vm.type_error(format!("Unknown signal: {name}"));
                    if let Value::Object(o) = &err {
                        o.set_own("code", "ERR_UNKNOWN_SIGNAL".into());
                    }
                    return Err(err);
                }
            }
        }
    };
    if let Some(e) = vm.io_children.get(&id) {
        let _ = e.cmd.send(ChildCmd::Kill(signal));
        return Ok(Value::Bool(true));
    }
    Ok(Value::Bool(false))
}

pub fn child_ref(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args)?;
    let want = arg(args, 1).truthy();
    let mut delta = 0i64;
    if let Some(e) = vm.io_children.get_mut(&id) {
        if e.refed != want {
            e.refed = want;
            delta = if want { 1 } else { -1 };
        }
    }
    adjust_pending(vm, delta);
    Ok(Value::Undefined)
}

pub fn flush_sockets(vm: &mut Vm) {
    let mut acks = Vec::new();
    for entry in vm.io_sockets.values() {
        let (tx, rx) = std::sync::mpsc::channel();
        if entry.cmd.send(SocketCmd::Flush(tx)).is_ok() {
            acks.push(rx);
        }
    }
    for rx in acks {
        let _ = rx.recv_timeout(std::time::Duration::from_millis(500));
    }
}

fn addr_values(addr: &SocketAddr) -> [Value; 2] {
    [
        addr.ip().to_string().into(),
        Value::Number(addr.port() as f64),
    ]
}

fn call_handler(vm: &mut Vm, handler: &Value, args: &[Value]) -> Result<(), Value> {
    if handler.is_callable() {
        vm.call_value(handler, Value::Undefined, args)?;
    }
    Ok(())
}

pub fn handle_socket(vm: &mut Vm, id: u64, event: SocketEvent) -> Result<(), Value> {
    let Some(entry) = vm.io_sockets.get(&id) else {
        return Ok(());
    };
    let handler = entry.handler.clone();
    match event {
        SocketEvent::Connected { local, remote } => {
            let [la, lp] = addr_values(&local);
            let [ra, rp] = addr_values(&remote);
            call_handler(vm, &handler, &["connect".into(), la, lp, ra, rp])
        }
        SocketEvent::Data(bytes) => {
            let chunk = super::typed::new_uint8(vm, bytes);
            call_handler(vm, &handler, &["data".into(), chunk])
        }
        SocketEvent::End => call_handler(vm, &handler, &["end".into()]),
        SocketEvent::Error { code, message } => {
            call_handler(vm, &handler, &["error".into(), message.into(), code.into()])
        }
        SocketEvent::Closed => {
            if let Some(e) = vm.io_sockets.remove(&id) {
                if e.refed {
                    adjust_pending(vm, -1);
                }
            }
            call_handler(vm, &handler, &["close".into()])
        }
    }
}

pub fn handle_accept(
    vm: &mut Vm,
    server: u64,
    id: u64,
    local: SocketAddr,
    remote: SocketAddr,
    cmd: Sender<SocketCmd>,
) -> Result<(), Value> {
    let Some(entry) = vm.io_net_servers.get(&server) else {
        let _ = cmd.send(SocketCmd::Destroy);
        return Ok(());
    };
    let handler = entry.handler.clone();
    vm.io_sockets.insert(
        id,
        SocketEntry {
            cmd,
            handler: Value::Undefined,
            refed: true,
        },
    );
    vm.io.pending += 1;
    let [la, lp] = addr_values(&local);
    let [ra, rp] = addr_values(&remote);
    call_handler(vm, &handler, &[Value::Number(id as f64), la, lp, ra, rp])
}

pub fn handle_child(vm: &mut Vm, id: u64, event: ChildEvent) -> Result<(), Value> {
    let Some(entry) = vm.io_children.get(&id) else {
        return Ok(());
    };
    let handler = entry.handler.clone();
    match event {
        ChildEvent::Stdout(bytes) => {
            let chunk = super::typed::new_uint8(vm, bytes);
            call_handler(vm, &handler, &["stdout".into(), chunk])
        }
        ChildEvent::Stderr(bytes) => {
            let chunk = super::typed::new_uint8(vm, bytes);
            call_handler(vm, &handler, &["stderr".into(), chunk])
        }
        ChildEvent::Error(message) => call_handler(vm, &handler, &["error".into(), message.into()]),
        ChildEvent::Exit { code, signal } => {
            if let Some(e) = vm.io_children.remove(&id) {
                if e.refed {
                    adjust_pending(vm, -1);
                }
            }
            let code = code.map(|c| Value::Number(c as f64)).unwrap_or(Value::Null);
            let signal = signal.map(Value::from).unwrap_or(Value::Null);
            call_handler(vm, &handler, &["exit".into(), code, signal])
        }
    }
}
