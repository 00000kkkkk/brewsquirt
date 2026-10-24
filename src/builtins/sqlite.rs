use super::{arg, method};
use crate::object::{Obj, ObjKind};
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use libsqlite3_sys as ffi;
use std::cell::{Cell, RefCell};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::rc::Rc;

type R = Result<Value, Value>;

struct Statement {
    handle: *mut ffi::sqlite3_stmt,
    db: usize,
}

thread_local! {
    static DATABASES: RefCell<Vec<Option<*mut ffi::sqlite3>>> = const { RefCell::new(Vec::new()) };
    static STATEMENTS: RefCell<Vec<Option<Statement>>> = const { RefCell::new(Vec::new()) };
    static CURRENT_VM: Cell<*mut Vm> = const { Cell::new(std::ptr::null_mut()) };
    static PENDING: RefCell<Option<Value>> = const { RefCell::new(None) };
}

fn coded(vm: &mut Vm, kind: ErrorKind, code: &str, message: String) -> Value {
    let e = vm.make_error(kind, message);
    if let Value::Object(o) = &e {
        o.set_own("code", code.into());
    }
    e
}

fn not_open(vm: &mut Vm) -> Value {
    coded(
        vm,
        ErrorKind::Error,
        "ERR_INVALID_STATE",
        "database is not open".into(),
    )
}

fn sqlite_error(vm: &mut Vm, db: *mut ffi::sqlite3, rc: c_int) -> Value {
    if let Some(e) = PENDING.with(|p| p.borrow_mut().take()) {
        return e;
    }
    let message = unsafe {
        if db.is_null() {
            CStr::from_ptr(ffi::sqlite3_errstr(rc))
                .to_string_lossy()
                .into_owned()
        } else {
            CStr::from_ptr(ffi::sqlite3_errmsg(db))
                .to_string_lossy()
                .into_owned()
        }
    };
    let errstr = unsafe {
        CStr::from_ptr(ffi::sqlite3_errstr(rc))
            .to_string_lossy()
            .into_owned()
    };
    let e = coded(vm, ErrorKind::Error, "ERR_SQLITE_ERROR", message);
    if let Value::Object(o) = &e {
        o.set_own("errcode", Value::Number(rc as f64));
        o.set_own("errstr", errstr.into());
    }
    e
}

fn id_arg(_vm: &mut Vm, args: &[Value], i: usize) -> Result<usize, Value> {
    Ok(match arg(args, i) {
        Value::Number(n) if n >= 0.0 => n as usize,
        _ => usize::MAX,
    })
}

fn database(vm: &mut Vm, id: usize) -> Result<*mut ffi::sqlite3, Value> {
    match DATABASES.with(|d| d.borrow().get(id).copied().flatten()) {
        Some(db) => Ok(db),
        None => Err(not_open(vm)),
    }
}

fn statement(vm: &mut Vm, id: usize) -> Result<(*mut ffi::sqlite3_stmt, usize), Value> {
    match STATEMENTS.with(|s| {
        s.borrow()
            .get(id)
            .and_then(|s| s.as_ref().map(|s| (s.handle, s.db)))
    }) {
        Some((h, db))
            if DATABASES
                .with(|d| d.borrow().get(db).copied().flatten())
                .is_some() =>
        {
            Ok((h, db))
        }
        Some(_) => Err(not_open(vm)),
        None => Err(coded(
            vm,
            ErrorKind::Error,
            "ERR_INVALID_STATE",
            "statement has been finalized".into(),
        )),
    }
}

fn cstring(vm: &mut Vm, s: &str) -> Result<CString, Value> {
    CString::new(s).map_err(|_| vm.type_error("SQL text must not contain NUL characters"))
}

struct VmScope(*mut Vm);

impl VmScope {
    fn enter(vm: &mut Vm) -> VmScope {
        VmScope(CURRENT_VM.with(|c| c.replace(vm as *mut Vm)))
    }
}

impl Drop for VmScope {
    fn drop(&mut self) {
        CURRENT_VM.with(|c| c.set(self.0));
    }
}

fn native_open(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let path = vm.to_string(&arg(args, 0))?.to_string();
    let read_only = arg(args, 1).truthy();
    let foreign_keys = arg(args, 2).truthy();
    let dqs = arg(args, 3).truthy();
    let timeout = vm.to_number(&arg(args, 4))?;
    let c_path = cstring(vm, &path)?;
    let flags = if read_only {
        ffi::SQLITE_OPEN_READONLY
    } else {
        ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE
    } | ffi::SQLITE_OPEN_URI;
    let mut db: *mut ffi::sqlite3 = std::ptr::null_mut();
    let rc = unsafe { ffi::sqlite3_open_v2(c_path.as_ptr(), &mut db, flags, std::ptr::null()) };
    if rc != ffi::SQLITE_OK {
        let e = sqlite_error(vm, db, rc);
        unsafe { ffi::sqlite3_close(db) };
        return Err(e);
    }
    unsafe {
        ffi::sqlite3_extended_result_codes(db, 1);
        let mut out: c_int = 0;
        ffi::sqlite3_db_config(
            db,
            ffi::SQLITE_DBCONFIG_ENABLE_FKEY,
            foreign_keys as c_int,
            &mut out as *mut c_int,
        );
        ffi::sqlite3_db_config(
            db,
            ffi::SQLITE_DBCONFIG_DQS_DML,
            dqs as c_int,
            &mut out as *mut c_int,
        );
        ffi::sqlite3_db_config(
            db,
            ffi::SQLITE_DBCONFIG_DQS_DDL,
            dqs as c_int,
            &mut out as *mut c_int,
        );
        if timeout > 0.0 {
            ffi::sqlite3_busy_timeout(db, timeout as c_int);
        }
    }
    let id = DATABASES.with(|d| {
        let mut d = d.borrow_mut();
        d.push(Some(db));
        d.len() - 1
    });
    Ok(Value::Number(id as f64))
}

fn native_close(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args, 0)?;
    let db = database(vm, id)?;
    STATEMENTS.with(|s| {
        for slot in s.borrow_mut().iter_mut() {
            if slot.as_ref().is_some_and(|st| st.db == id)
                && let Some(st) = slot.take()
            {
                unsafe { ffi::sqlite3_finalize(st.handle) };
            }
        }
    });
    let rc = unsafe { ffi::sqlite3_close(db) };
    DATABASES.with(|d| d.borrow_mut()[id] = None);
    if rc != ffi::SQLITE_OK {
        return Err(sqlite_error(vm, std::ptr::null_mut(), rc));
    }
    Ok(Value::Undefined)
}

fn native_exec(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let db = {
        let id = id_arg(vm, args, 0)?;
        database(vm, id)?
    };
    let sql = vm.to_string(&arg(args, 1))?.to_string();
    let c_sql = cstring(vm, &sql)?;
    let _scope = VmScope::enter(vm);
    let rc = unsafe {
        ffi::sqlite3_exec(
            db,
            c_sql.as_ptr(),
            None,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if rc != ffi::SQLITE_OK {
        return Err(sqlite_error(vm, db, rc));
    }
    Ok(Value::Undefined)
}

fn native_prepare(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let db_id = id_arg(vm, args, 0)?;
    let db = database(vm, db_id)?;
    let sql = vm.to_string(&arg(args, 1))?.to_string();
    let c_sql = cstring(vm, &sql)?;
    let mut stmt: *mut ffi::sqlite3_stmt = std::ptr::null_mut();
    let rc =
        unsafe { ffi::sqlite3_prepare_v2(db, c_sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut()) };
    if rc != ffi::SQLITE_OK {
        return Err(sqlite_error(vm, db, rc));
    }
    if stmt.is_null() {
        return Err(coded(
            vm,
            ErrorKind::Error,
            "ERR_INVALID_STATE",
            "The supplied SQL string contains no statements".into(),
        ));
    }
    let id = STATEMENTS.with(|s| {
        let mut s = s.borrow_mut();
        s.push(Some(Statement {
            handle: stmt,
            db: db_id,
        }));
        s.len() - 1
    });
    Ok(Value::Number(id as f64))
}

fn native_finalize(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = id_arg(vm, args, 0)?;
    if let Some(st) = STATEMENTS.with(|s| s.borrow_mut().get_mut(id).and_then(Option::take)) {
        unsafe { ffi::sqlite3_finalize(st.handle) };
    }
    Ok(Value::Undefined)
}

fn bind_value(
    vm: &mut Vm,
    stmt: *mut ffi::sqlite3_stmt,
    index: c_int,
    v: &Value,
) -> Result<(), Value> {
    let rc = unsafe {
        match v {
            Value::Null => ffi::sqlite3_bind_null(stmt, index),
            Value::Number(n) => ffi::sqlite3_bind_double(stmt, index, *n),
            Value::BigInt(b) => {
                let Ok(i) = i64::try_from(&**b) else {
                    return Err(coded(
                        vm,
                        ErrorKind::RangeError,
                        "ERR_INVALID_ARG_VALUE",
                        format!("BigInt value is too large to bind to SQLite parameter {index}."),
                    ));
                };
                ffi::sqlite3_bind_int64(stmt, index, i)
            }
            Value::Str(s) => {
                let text = s.to_string();
                ffi::sqlite3_bind_text(
                    stmt,
                    index,
                    text.as_ptr() as *const c_char,
                    text.len() as c_int,
                    ffi::SQLITE_TRANSIENT(),
                )
            }
            other => match super::binary::buffer_source_bytes(other) {
                Some(bytes) if other.is_object() => ffi::sqlite3_bind_blob(
                    stmt,
                    index,
                    bytes.as_ptr() as *const c_void,
                    bytes.len() as c_int,
                    ffi::SQLITE_TRANSIENT(),
                ),
                _ => {
                    return Err(coded(
                        vm,
                        ErrorKind::TypeError,
                        "ERR_INVALID_ARG_TYPE",
                        format!("Provided value cannot be bound to SQLite parameter {index}."),
                    ));
                }
            },
        }
    };
    if rc != ffi::SQLITE_OK {
        return Err(sqlite_error(
            vm,
            unsafe { ffi::sqlite3_db_handle(stmt) },
            rc,
        ));
    }
    Ok(())
}

fn is_named_object(v: &Value) -> bool {
    match v {
        Value::Object(o) => {
            matches!(o.kind, ObjKind::Ordinary) && super::binary::buffer_source_bytes(v).is_none()
        }
        _ => false,
    }
}

fn native_bind(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let (stmt, _) = {
        let id = id_arg(vm, args, 0)?;
        statement(vm, id)?
    };
    let params = match arg(args, 1) {
        Value::Object(o) => match &o.kind {
            ObjKind::Array(a) => a.borrow().clone(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    let allow_bare = arg(args, 2).truthy();
    let allow_unknown = arg(args, 3).truthy();
    unsafe {
        ffi::sqlite3_reset(stmt);
        ffi::sqlite3_clear_bindings(stmt);
    }
    let count = unsafe { ffi::sqlite3_bind_parameter_count(stmt) };
    let mut positional = params.as_slice();
    if let Some(first) = params.first().filter(|v| is_named_object(v)) {
        positional = &params[1..];
        let mut seen = Vec::new();
        for i in 1..=count {
            let name = unsafe { ffi::sqlite3_bind_parameter_name(stmt, i) };
            if name.is_null() {
                continue;
            }
            let full = unsafe { CStr::from_ptr(name).to_string_lossy().into_owned() };
            let bare = full[1..].to_string();
            let key = if vm.has_property(first, &Value::from(full.as_str()))? {
                Some(full.clone())
            } else if allow_bare && vm.has_property(first, &Value::from(bare.as_str()))? {
                Some(bare.clone())
            } else {
                None
            };
            if let Some(k) = key {
                let v = vm.get_prop(first, &k)?;
                bind_value(vm, stmt, i, &v)?;
                seen.push(k);
            }
        }
        if !allow_unknown && first.is_object() {
            for (k, _) in vm.own_entries(first) {
                if !seen.iter().any(|s| **s == *k) {
                    return Err(coded(
                        vm,
                        ErrorKind::Error,
                        "ERR_INVALID_STATE",
                        format!("Unknown named parameter '{k}'"),
                    ));
                }
            }
        }
    }
    let mut next = positional.iter();
    for i in 1..=count {
        let name = unsafe { ffi::sqlite3_bind_parameter_name(stmt, i) };
        let anonymous = name.is_null() || unsafe { *name } == b'?' as c_char;
        if !anonymous {
            continue;
        }
        if let Some(v) = next.next() {
            bind_value(vm, stmt, i, v)?;
        }
    }
    Ok(Value::Undefined)
}

fn column_value(
    vm: &mut Vm,
    stmt: *mut ffi::sqlite3_stmt,
    i: c_int,
    big: bool,
) -> Result<Value, Value> {
    unsafe {
        Ok(match ffi::sqlite3_column_type(stmt, i) {
            ffi::SQLITE_INTEGER => {
                let n = ffi::sqlite3_column_int64(stmt, i);
                integer_value(vm, n, big)?
            }
            ffi::SQLITE_FLOAT => Value::Number(ffi::sqlite3_column_double(stmt, i)),
            ffi::SQLITE_TEXT => {
                let p = ffi::sqlite3_column_text(stmt, i);
                let len = ffi::sqlite3_column_bytes(stmt, i) as usize;
                let bytes = if p.is_null() {
                    &[][..]
                } else {
                    std::slice::from_raw_parts(p, len)
                };
                Value::from(String::from_utf8_lossy(bytes).into_owned())
            }
            ffi::SQLITE_BLOB => {
                let p = ffi::sqlite3_column_blob(stmt, i) as *const u8;
                let len = ffi::sqlite3_column_bytes(stmt, i) as usize;
                let bytes = if p.is_null() {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(p, len).to_vec()
                };
                super::typed::new_uint8(vm, bytes)
            }
            _ => Value::Null,
        })
    }
}

fn integer_value(vm: &mut Vm, n: i64, big: bool) -> Result<Value, Value> {
    if big {
        return Ok(Value::BigInt(Rc::new(num_bigint::BigInt::from(n))));
    }
    if n.unsigned_abs() > 9007199254740991 {
        return Err(coded(
            vm,
            ErrorKind::RangeError,
            "ERR_OUT_OF_RANGE",
            format!("Value is too large to be represented as a JavaScript number: {n}"),
        ));
    }
    Ok(Value::Number(n as f64))
}

fn native_step(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let (stmt, db_id) = {
        let id = id_arg(vm, args, 0)?;
        statement(vm, id)?
    };
    let big = arg(args, 1).truthy();
    let arrays = arg(args, 2).truthy();
    let rc = {
        let _scope = VmScope::enter(vm);
        unsafe { ffi::sqlite3_step(stmt) }
    };
    match rc {
        ffi::SQLITE_ROW => {
            let n = unsafe { ffi::sqlite3_column_count(stmt) };
            if arrays {
                let mut items = Vec::with_capacity(n as usize);
                for i in 0..n {
                    items.push(column_value(vm, stmt, i, big)?);
                }
                return Ok(vm.new_array(items));
            }
            let row = Obj::new(ObjKind::Ordinary, None);
            for i in 0..n {
                let name = unsafe { ffi::sqlite3_column_name(stmt, i) };
                let name = if name.is_null() {
                    String::new()
                } else {
                    unsafe { CStr::from_ptr(name).to_string_lossy().into_owned() }
                };
                let v = column_value(vm, stmt, i, big)?;
                row.set_own(name, v);
            }
            Ok(Value::Object(row))
        }
        ffi::SQLITE_DONE => Ok(Value::Undefined),
        _ => {
            let db = database(vm, db_id)?;
            let e = sqlite_error(vm, db, rc);
            unsafe { ffi::sqlite3_reset(stmt) };
            Err(e)
        }
    }
}

fn native_reset(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let (stmt, _) = {
        let id = id_arg(vm, args, 0)?;
        statement(vm, id)?
    };
    unsafe { ffi::sqlite3_reset(stmt) };
    Ok(Value::Undefined)
}

fn native_changes(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let db = {
        let id = id_arg(vm, args, 0)?;
        database(vm, id)?
    };
    let big = arg(args, 1).truthy();
    let (changes, rowid) = unsafe {
        (
            ffi::sqlite3_changes64(db),
            ffi::sqlite3_last_insert_rowid(db),
        )
    };
    let o = vm.new_object();
    let changes = integer_value(vm, changes, big)?;
    let rowid = integer_value(vm, rowid, big)?;
    o.set_own("changes", changes);
    o.set_own("lastInsertRowid", rowid);
    Ok(Value::Object(o))
}

fn text_or_null(p: *const c_char) -> Value {
    if p.is_null() {
        Value::Null
    } else {
        Value::from(unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() })
    }
}

fn native_columns(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let (stmt, _) = {
        let id = id_arg(vm, args, 0)?;
        statement(vm, id)?
    };
    let n = unsafe { ffi::sqlite3_column_count(stmt) };
    let mut out = Vec::new();
    for i in 0..n {
        let c = Obj::new(ObjKind::Ordinary, None);
        unsafe {
            c.set_own(
                "column",
                text_or_null(ffi::sqlite3_column_origin_name(stmt, i)),
            );
            c.set_own(
                "database",
                text_or_null(ffi::sqlite3_column_database_name(stmt, i)),
            );
            c.set_own("name", text_or_null(ffi::sqlite3_column_name(stmt, i)));
            c.set_own(
                "table",
                text_or_null(ffi::sqlite3_column_table_name(stmt, i)),
            );
            c.set_own("type", text_or_null(ffi::sqlite3_column_decltype(stmt, i)));
        }
        out.push(Value::Object(c));
    }
    Ok(vm.new_array(out))
}

fn native_sql(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let (stmt, _) = {
        let id = id_arg(vm, args, 0)?;
        statement(vm, id)?
    };
    if arg(args, 1).truthy() {
        let p = unsafe { ffi::sqlite3_expanded_sql(stmt) };
        let v = text_or_null(p);
        unsafe { ffi::sqlite3_free(p as *mut c_void) };
        return Ok(v);
    }
    Ok(text_or_null(unsafe { ffi::sqlite3_sql(stmt) }))
}

fn native_in_transaction(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let db = {
        let id = id_arg(vm, args, 0)?;
        database(vm, id)?
    };
    Ok(Value::Bool(unsafe { ffi::sqlite3_get_autocommit(db) } == 0))
}

fn native_location(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let db = {
        let id = id_arg(vm, args, 0)?;
        database(vm, id)?
    };
    let name = vm.to_string(&arg(args, 1))?.to_string();
    let c_name = cstring(vm, &name)?;
    let p = unsafe { ffi::sqlite3_db_filename(db, c_name.as_ptr()) };
    Ok(match text_or_null(p) {
        Value::Str(s) if s.is_empty() => Value::Null,
        other => other,
    })
}

struct UserFunction {
    func: Value,
    big: bool,
}

unsafe fn sqlite_value(vm: &mut Vm, v: *mut ffi::sqlite3_value, big: bool) -> Result<Value, Value> {
    unsafe {
        Ok(match ffi::sqlite3_value_type(v) {
            ffi::SQLITE_INTEGER => integer_value(vm, ffi::sqlite3_value_int64(v), big)?,
            ffi::SQLITE_FLOAT => Value::Number(ffi::sqlite3_value_double(v)),
            ffi::SQLITE_TEXT => {
                let p = ffi::sqlite3_value_text(v);
                let len = ffi::sqlite3_value_bytes(v) as usize;
                let bytes = if p.is_null() {
                    &[][..]
                } else {
                    std::slice::from_raw_parts(p, len)
                };
                Value::from(String::from_utf8_lossy(bytes).into_owned())
            }
            ffi::SQLITE_BLOB => {
                let p = ffi::sqlite3_value_blob(v) as *const u8;
                let len = ffi::sqlite3_value_bytes(v) as usize;
                let bytes = if p.is_null() {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(p, len).to_vec()
                };
                super::typed::new_uint8(vm, bytes)
            }
            _ => Value::Null,
        })
    }
}

unsafe fn report(ctx: *mut ffi::sqlite3_context, e: Value) {
    let message = CString::new("user-defined function raised an exception").unwrap_or_default();
    PENDING.with(|p| *p.borrow_mut() = Some(e));
    unsafe { ffi::sqlite3_result_error(ctx, message.as_ptr(), -1) };
}

unsafe extern "C" fn call_user_function(
    ctx: *mut ffi::sqlite3_context,
    argc: c_int,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let vm = CURRENT_VM.with(Cell::get);
    let data = unsafe { ffi::sqlite3_user_data(ctx) as *const UserFunction };
    if vm.is_null() || data.is_null() {
        let message = c"user-defined function called outside of a statement";
        unsafe { ffi::sqlite3_result_error(ctx, message.as_ptr(), -1) };
        return;
    }
    let vm = unsafe { &mut *vm };
    let f = unsafe { &*data };
    let mut values = Vec::with_capacity(argc.max(0) as usize);
    for i in 0..argc.max(0) as usize {
        match unsafe { sqlite_value(vm, *argv.add(i), f.big) } {
            Ok(v) => values.push(v),
            Err(e) => {
                unsafe { report(ctx, e) };
                return;
            }
        }
    }
    let result = match vm.call_value(&f.func, Value::Undefined, &values) {
        Ok(v) => v,
        Err(e) => {
            unsafe { report(ctx, e) };
            return;
        }
    };
    unsafe {
        match &result {
            Value::Null | Value::Undefined => ffi::sqlite3_result_null(ctx),
            Value::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 9007199254740992.0 {
                    ffi::sqlite3_result_int64(ctx, *n as i64)
                } else {
                    ffi::sqlite3_result_double(ctx, *n)
                }
            }
            Value::Bool(b) => ffi::sqlite3_result_int64(ctx, *b as i64),
            Value::BigInt(b) => match i64::try_from(&**b) {
                Ok(i) => ffi::sqlite3_result_int64(ctx, i),
                Err(_) => {
                    let e = vm.range_error("BigInt value is too large for SQLite");
                    report(ctx, e);
                }
            },
            Value::Str(s) => {
                let text = s.to_string();
                ffi::sqlite3_result_text(
                    ctx,
                    text.as_ptr() as *const c_char,
                    text.len() as c_int,
                    ffi::SQLITE_TRANSIENT(),
                )
            }
            other => match super::binary::buffer_source_bytes(other) {
                Some(bytes) => ffi::sqlite3_result_blob(
                    ctx,
                    bytes.as_ptr() as *const c_void,
                    bytes.len() as c_int,
                    ffi::SQLITE_TRANSIENT(),
                ),
                None => {
                    let e = vm.type_error(
                        "Returned JavaScript value cannot be converted to a SQLite value",
                    );
                    report(ctx, e);
                }
            },
        }
    }
}

unsafe extern "C" fn drop_user_function(p: *mut c_void) {
    if !p.is_null() {
        drop(unsafe { Box::from_raw(p as *mut UserFunction) });
    }
}

fn native_function(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let db = {
        let id = id_arg(vm, args, 0)?;
        database(vm, id)?
    };
    let name = vm.to_string(&arg(args, 1))?.to_string();
    let func = arg(args, 2);
    if !func.is_callable() {
        return Err(vm.type_error("The \"function\" argument must be a function"));
    }
    let deterministic = arg(args, 3).truthy();
    let direct_only = arg(args, 4).truthy();
    let varargs = arg(args, 5).truthy();
    let big = arg(args, 6).truthy();
    let length = vm.get_prop(&func, "length")?;
    let n_args = if varargs {
        -1
    } else {
        vm.to_number(&length)? as c_int
    };
    let c_name = cstring(vm, &name)?;
    let mut flags = ffi::SQLITE_UTF8;
    if deterministic {
        flags |= ffi::SQLITE_DETERMINISTIC;
    }
    if direct_only {
        flags |= ffi::SQLITE_DIRECTONLY;
    }
    let data = Box::into_raw(Box::new(UserFunction { func, big }));
    let rc = unsafe {
        ffi::sqlite3_create_function_v2(
            db,
            c_name.as_ptr(),
            n_args,
            flags,
            data as *mut c_void,
            Some(call_user_function),
            None,
            None,
            Some(drop_user_function),
        )
    };
    if rc != ffi::SQLITE_OK {
        return Err(sqlite_error(vm, db, rc));
    }
    Ok(Value::Undefined)
}

const SQLITE_SRC: &str = r##"
(native) => {
  const kDb = Symbol("database")
  const kStmt = Symbol("statement")
  const kOpts = Symbol("options")
  const invalidState = (message) => Object.assign(new Error(message), { code: "ERR_INVALID_STATE" })
  const typeError = (message) => Object.assign(new TypeError(message), { code: "ERR_INVALID_ARG_TYPE" })
  const pathString = (p) => {
    if (typeof p === "string") return p
    if (p instanceof URL) {
      if (p.protocol !== "file:") throw typeError('The "path" argument must be a file URL object, a file URL string, a Buffer, or a string.')
      return p.href
    }
    if (p instanceof Uint8Array) return new TextDecoder().decode(p)
    throw typeError('The "path" argument must be a string, Uint8Array, or URL without null bytes.')
  }
  class DatabaseSync {
    #id = null
    #path
    #options
    constructor(path, options = {}) {
      if (new.target === undefined) throw new TypeError("Class constructor DatabaseSync cannot be invoked without 'new'")
      this.#path = pathString(path)
      if (options === null || typeof options !== "object") throw typeError('The "options" argument must be an object.')
      this.#options = {
        open: options.open ?? true,
        readOnly: !!options.readOnly,
        enableForeignKeyConstraints: options.enableForeignKeyConstraints ?? true,
        enableDoubleQuotedStringLiterals: !!options.enableDoubleQuotedStringLiterals,
        timeout: options.timeout ?? 0,
        readBigInts: !!options.readBigInts,
        returnArrays: !!options.returnArrays,
        allowBareNamedParameters: options.allowBareNamedParameters ?? true,
        allowUnknownNamedParameters: !!options.allowUnknownNamedParameters,
      }
      if (this.#options.open) this.open()
    }
    open() {
      if (this.#id !== null) throw invalidState("database is already open")
      const o = this.#options
      this.#id = native.open(this.#path, o.readOnly, o.enableForeignKeyConstraints, o.enableDoubleQuotedStringLiterals, o.timeout)
    }
    get isOpen() {
      return this.#id !== null
    }
    get isTransaction() {
      return native.inTransaction(this.#check())
    }
    #check() {
      if (this.#id === null) throw invalidState("database is not open")
      return this.#id
    }
    close() {
      native.close(this.#check())
      this.#id = null
    }
    exec(sql) {
      if (typeof sql !== "string") throw typeError('The "sql" argument must be a string.')
      native.exec(this.#check(), sql)
    }
    prepare(sql) {
      if (typeof sql !== "string") throw typeError('The "sql" argument must be a string.')
      const id = native.prepare(this.#check(), sql)
      return makeStatement(this.#id, id, this.#options)
    }
    function(name, options, fn) {
      if (typeof options === "function") [options, fn] = [{}, options]
      if (typeof name !== "string") throw typeError('The "name" argument must be a string.')
      if (typeof fn !== "function") throw typeError('The "function" argument must be a function.')
      const o = options ?? {}
      native.function(this.#check(), name, fn, !!o.deterministic, !!o.directOnly, !!o.varargs, !!o.useBigIntArguments)
    }
    location(dbName = "main") {
      return native.location(this.#check(), dbName)
    }
    enableLoadExtension() {
      throw invalidState("extension loading is not supported")
    }
    loadExtension() {
      throw invalidState("extension loading is not supported")
    }
    [Symbol.dispose]() {
      if (this.#id !== null) this.close()
    }
  }
  const STATEMENT_KEY = Symbol("construct")
  class StatementSync {
    constructor(key, db, id, options) {
      if (key !== STATEMENT_KEY) throw Object.assign(new TypeError("Illegal constructor"), { code: "ERR_ILLEGAL_CONSTRUCTOR" })
      this[kDb] = db
      this[kStmt] = id
      this[kOpts] = {
        readBigInts: options.readBigInts,
        returnArrays: options.returnArrays,
        allowBare: options.allowBareNamedParameters,
        allowUnknown: options.allowUnknownNamedParameters,
      }
    }
    #bind(args) {
      const o = this[kOpts]
      native.bind(this[kStmt], args, o.allowBare, o.allowUnknown)
    }
    run(...args) {
      this.#bind(args)
      try {
        while (native.step(this[kStmt], false, true) !== undefined) {}
      } finally {
        native.reset(this[kStmt])
      }
      return native.changes(this[kDb], this[kOpts].readBigInts)
    }
    get(...args) {
      this.#bind(args)
      try {
        return native.step(this[kStmt], this[kOpts].readBigInts, this[kOpts].returnArrays)
      } finally {
        native.reset(this[kStmt])
      }
    }
    all(...args) {
      this.#bind(args)
      const rows = []
      try {
        let row
        while ((row = native.step(this[kStmt], this[kOpts].readBigInts, this[kOpts].returnArrays)) !== undefined) rows.push(row)
      } finally {
        native.reset(this[kStmt])
      }
      return rows
    }
    iterate(...args) {
      this.#bind(args)
      const id = this[kStmt]
      const o = this[kOpts]
      let done = false
      const it = {
        next() {
          if (done) return { value: undefined, done: true }
          let row
          try {
            row = native.step(id, o.readBigInts, o.returnArrays)
          } catch (e) {
            done = true
            native.reset(id)
            throw e
          }
          if (row === undefined) {
            done = true
            native.reset(id)
            return { value: undefined, done: true }
          }
          return { value: row, done: false }
        },
        return() {
          if (!done) {
            done = true
            native.reset(id)
          }
          return { value: undefined, done: true }
        },
        [Symbol.iterator]() {
          return this
        },
      }
      Object.setPrototypeOf(it, Object.getPrototypeOf(Object.getPrototypeOf([][Symbol.iterator]())))
      return it
    }
    columns() {
      return native.columns(this[kStmt])
    }
    get sourceSQL() {
      return native.sql(this[kStmt], false)
    }
    get expandedSQL() {
      return native.sql(this[kStmt], true)
    }
    setReadBigInts(v) {
      if (typeof v !== "boolean") throw typeError('The "readBigInts" argument must be a boolean.')
      this[kOpts].readBigInts = v
    }
    setReturnArrays(v) {
      if (typeof v !== "boolean") throw typeError('The "returnArrays" argument must be a boolean.')
      this[kOpts].returnArrays = v
    }
    setAllowBareNamedParameters(v) {
      if (typeof v !== "boolean") throw typeError('The "allowBareNamedParameters" argument must be a boolean.')
      this[kOpts].allowBare = v
    }
    setAllowUnknownNamedParameters(v) {
      if (typeof v !== "boolean") throw typeError('The "enabled" argument must be a boolean.')
      this[kOpts].allowUnknown = v
    }
  }
  const makeStatement = (db, id, options) => new StatementSync(STATEMENT_KEY, db, id, options)
  const constants = {
    SQLITE_CHANGESET_OMIT: 0,
    SQLITE_CHANGESET_REPLACE: 1,
    SQLITE_CHANGESET_ABORT: 2,
    SQLITE_CHANGESET_DATA: 1,
    SQLITE_CHANGESET_NOTFOUND: 2,
    SQLITE_CHANGESET_CONFLICT: 3,
    SQLITE_CHANGESET_CONSTRAINT: 4,
    SQLITE_CHANGESET_FOREIGN_KEY: 5,
  }
  const sqlite = { DatabaseSync, StatementSync, constants }
  Object.defineProperty(sqlite, "default", { value: sqlite, enumerable: false })
  return sqlite
}
"##;

pub fn load(vm: &mut Vm) -> R {
    let dir = std::env::current_dir().unwrap_or_default();
    let proto = match vm.compile(
        SQLITE_SRC,
        "<node:sqlite>",
        dir,
        crate::compiler::Mode::Repl,
    ) {
        Ok(p) => p,
        Err(e) => return Err(vm.syntax_error_value(&e, "<node:sqlite>")),
    };
    let factory = vm.run_proto(proto)?;
    let n = vm.new_object();
    let natives: [(&str, u32, crate::object::NativeFn); 15] = [
        ("open", 5, native_open),
        ("close", 1, native_close),
        ("exec", 2, native_exec),
        ("prepare", 2, native_prepare),
        ("finalize", 1, native_finalize),
        ("bind", 4, native_bind),
        ("step", 3, native_step),
        ("reset", 1, native_reset),
        ("changes", 2, native_changes),
        ("columns", 1, native_columns),
        ("sql", 2, native_sql),
        ("inTransaction", 1, native_in_transaction),
        ("location", 2, native_location),
        ("function", 7, native_function),
        ("version", 0, native_version),
    ];
    for (name, arity, f) in natives {
        method(vm, &n, name, arity, f);
    }
    vm.call_value(&factory, Value::Undefined, &[Value::Object(n)])
}

fn native_version(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(Value::from(
        unsafe { CStr::from_ptr(ffi::sqlite3_libversion()) }
            .to_string_lossy()
            .into_owned(),
    ))
}
