use super::{arg, make_ctor, method, native_fn};
use crate::compiler::Mode;
use crate::inspect::inspect_plain;
use crate::object::{Obj, ObjKind, PromiseData};
use crate::value::Value;
use crate::vm::Vm;
use std::cell::RefCell;
use std::rc::Rc;

type R = Result<Value, Value>;

const PRELUDE: &str = r#"
const P = Promise

P.prototype.finally = function (onFinally) {
  const run = () => (typeof onFinally === "function" ? onFinally() : undefined)
  return this.then(
    (value) => P.resolve(run()).then(() => value),
    (reason) =>
      P.resolve(run()).then(() => {
        throw reason
      }),
  )
}

P.all = function (items) {
  return new P((resolve, reject) => {
    const list = [...items]
    const out = new Array(list.length)
    let left = list.length
    if (left === 0) resolve(out)
    list.forEach((item, i) => {
      P.resolve(item).then((value) => {
        out[i] = value
        left -= 1
        if (left === 0) resolve(out)
      }, reject)
    })
  })
}

P.allSettled = function (items) {
  return P.all(
    [...items].map((item) =>
      P.resolve(item).then(
        (value) => ({ status: "fulfilled", value }),
        (reason) => ({ status: "rejected", reason }),
      ),
    ),
  )
}

P.race = function (items) {
  return new P((resolve, reject) => {
    for (const item of items) P.resolve(item).then(resolve, reject)
  })
}

P.any = function (items) {
  return new P((resolve, reject) => {
    const list = [...items]
    const errors = new Array(list.length)
    let left = list.length
    const fail = () => {
      reject(new AggregateError(errors, "All promises were rejected"))
    }
    if (left === 0) fail()
    list.forEach((item, i) => {
      P.resolve(item).then(resolve, (reason) => {
        errors[i] = reason
        left -= 1
        if (left === 0) fail()
      })
    })
  })
}

P.withResolvers = function () {
  let resolve
  let reject
  const promise = new P((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}
"#;

pub fn install(vm: &mut Vm) {
    let pp = vm.realm.promise_proto.clone();
    method(vm, &pp, "then", 2, then);
    method(vm, &pp, "catch", 1, catch);
    let promise = make_ctor(
        vm,
        "Promise",
        1,
        promise_ctor,
        Some(|| ObjKind::Promise(Box::new(RefCell::new(PromiseData::default())))),
        &pp,
    );
    method(vm, &promise, "resolve", 1, promise_resolve_static);
    method(vm, &promise, "reject", 1, promise_reject_static);
    vm.define_global("Promise", Value::Object(promise));
    vm.promise_resolve_fn = native_fn(vm, "resolve", 1, resolve_fn);
    vm.promise_reject_fn = native_fn(vm, "reject", 1, reject_fn);
}

pub fn run_prelude(vm: &mut Vm) {
    let dir = std::env::current_dir().unwrap_or_default();
    let proto = vm
        .compile(PRELUDE, "<prelude>", dir, Mode::Module)
        .expect("prelude compiles");
    if let Err(e) = vm.run_proto(proto) {
        panic!("prelude failed: {}", inspect_plain(&e));
    }
}

fn this_promise(vm: &mut Vm, this: &Value) -> Result<Rc<Obj>, Value> {
    match this {
        Value::Object(o) if matches!(o.kind, ObjKind::Promise(_)) => Ok(o.clone()),
        _ => Err(vm.type_error(format!(
            "Method Promise.prototype.then called on incompatible receiver {}",
            inspect_plain(this)
        ))),
    }
}

fn promise_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let p = match &this {
        Value::Object(o) if matches!(o.kind, ObjKind::Promise(_)) => o.clone(),
        _ => return Err(vm.type_error("Promise constructor cannot be invoked without 'new'")),
    };
    let executor = arg(args, 0);
    if !executor.is_callable() {
        return Err(vm.type_error(format!(
            "Promise resolver {} is not a function",
            inspect_plain(&executor)
        )));
    }
    let (resolve, reject) = vm.resolving_functions(&p);
    if let Err(e) = vm.call_value(&executor, Value::Undefined, &[resolve, reject]) {
        vm.reject_promise(&p, e);
    }
    Ok(this)
}

fn then(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let p = this_promise(vm, &this)?;
    let derived = vm.promise_then(&p, arg(args, 0), arg(args, 1));
    Ok(Value::Object(derived))
}

fn catch(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let p = this_promise(vm, &this)?;
    let derived = vm.promise_then(&p, Value::Undefined, arg(args, 0));
    Ok(Value::Object(derived))
}

fn promise_resolve_static(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Object(vm.to_promise(arg(args, 0))))
}

fn promise_reject_static(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = vm.new_promise();
    vm.reject_promise(&p, arg(args, 0));
    Ok(Value::Object(p))
}

fn resolve_fn(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if let Value::Object(p) = &this {
        vm.resolve_promise(p, arg(args, 0));
    }
    Ok(Value::Undefined)
}

fn reject_fn(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if let Value::Object(p) = &this {
        vm.reject_promise(p, arg(args, 0));
    }
    Ok(Value::Undefined)
}
