use super::{arg, make_ctor, method, native_fn};
use crate::compiler::Mode;
use crate::inspect::inspect_plain;
use crate::object::{Obj, ObjKind, PromiseData};
use crate::value::Value;
use crate::vm::Vm;
use std::cell::RefCell;
use std::rc::Rc;

type R = Result<Value, Value>;

const PRELUDE: &str = r##"
const P = Promise
const thenRaw = globalThis["%PromiseThenRaw%"]
const isCtor = globalThis["%IsConstructor%"]
const isPromise = globalThis["%IsPromise%"]
const isObject = (v) => v !== null && (typeof v === "object" || typeof v === "function")
const DONE = Symbol("done")

const capabilityExecutor = (record) => (resolve, reject) => {
  if (record.resolve !== undefined) throw new TypeError("Promise executor has already been invoked with non-undefined arguments")
  if (record.reject !== undefined) throw new TypeError("Promise executor has already been invoked with non-undefined arguments")
  record.resolve = resolve
  record.reject = reject
}

const newCapability = (C) => {
  if (!isCtor(C)) throw new TypeError("Promise capability target is not a constructor")
  const record = { promise: undefined, resolve: undefined, reject: undefined }
  record.promise = new C(capabilityExecutor(record))
  if (typeof record.resolve !== "function") throw new TypeError("Promise resolve function is not callable")
  if (typeof record.reject !== "function") throw new TypeError("Promise reject function is not callable")
  return record
}

const speciesOf = (O) => {
  const C = O.constructor
  if (C === undefined) return P
  if (!isObject(C)) throw new TypeError("object.constructor is not an object")
  const S = C[Symbol.species]
  if (S === undefined || S === null) return P
  if (isCtor(S)) return S
  throw new TypeError("object.constructor[Symbol.species] is not a constructor")
}

const resolveWith = (C, x) => {
  if (isPromise(x)) {
    const xc = x.constructor
    if (xc === C) return x
  }
  const cap = newCapability(C)
  const resolve = cap.resolve
  resolve(x)
  return cap.promise
}

const getIterator = (obj) => {
  const method = obj == null ? undefined : obj[Symbol.iterator]
  if (typeof method !== "function") throw new TypeError(`${typeof obj === "string" ? obj : typeof obj} is not iterable`)
  const it = method.call(obj)
  if (!isObject(it)) throw new TypeError("Result of the Symbol.iterator method is not an object")
  return { it, next: it.next, done: false }
}

const stepValue = (rec) => {
  let result
  try {
    result = rec.next.call(rec.it)
  } catch (e) {
    rec.done = true
    throw e
  }
  if (!isObject(result)) {
    rec.done = true
    throw new TypeError("Iterator result is not an object")
  }
  let done
  try {
    done = result.done
  } catch (e) {
    rec.done = true
    throw e
  }
  if (done) {
    rec.done = true
    return DONE
  }
  try {
    return result.value
  } catch (e) {
    rec.done = true
    throw e
  }
}

const closeQuietly = (rec) => {
  try {
    const ret = rec.it.return
    if (ret !== undefined && ret !== null) ret.call(rec.it)
  } catch {}
}

const getResolve = (C) => {
  const r = C.resolve
  if (typeof r !== "function") throw new TypeError("Promise resolve or reject function is not callable")
  return r
}

const combinator = (name, perform) => {
  const fn = {
    [name](iterable) {
      const C = this
      const cap = newCapability(C)
      const reject = cap.reject
      let promiseResolve
      let rec
      try {
        promiseResolve = getResolve(C)
        rec = getIterator(iterable)
      } catch (e) {
        reject(e)
        return cap.promise
      }
      try {
        return perform(rec, C, cap, promiseResolve)
      } catch (e) {
        if (!rec.done) closeQuietly(rec)
        reject(e)
        return cap.promise
      }
    },
  }[name]
  return fn
}

const allResolver = (values, index, remaining, cap) => {
  let called = false
  return (x) => {
    if (called) return undefined
    called = true
    values[index] = x
    if (--remaining.n === 0) {
      const resolve = cap.resolve
      resolve(values.slice())
    }
    return undefined
  }
}

const settledResolver = (values, index, remaining, cap, flag, rejected) => {
  return (x) => {
    if (flag.called) return undefined
    flag.called = true
    values[index] = rejected ? { status: "rejected", reason: x } : { status: "fulfilled", value: x }
    if (--remaining.n === 0) {
      const resolve = cap.resolve
      resolve(values.slice())
    }
    return undefined
  }
}

const anyRejecter = (errors, index, remaining, cap) => {
  let called = false
  return (x) => {
    if (called) return undefined
    called = true
    errors[index] = x
    if (--remaining.n === 0) {
      const error = new AggregateError([], "All promises were rejected")
      Object.defineProperty(error, "errors", { value: errors.slice(), writable: true, enumerable: false, configurable: true })
      const reject = cap.reject
      reject(error)
    }
    return undefined
  }
}

P.all = combinator("all", (rec, C, cap, promiseResolve) => {
  const values = []
  const remaining = { n: 1 }
  let index = 0
  for (;;) {
    const next = stepValue(rec)
    if (next === DONE) {
      if (--remaining.n === 0) {
        const resolve = cap.resolve
        resolve(values.slice())
      }
      return cap.promise
    }
    values.push(undefined)
    const nextPromise = promiseResolve.call(C, next)
    remaining.n++
    nextPromise.then(allResolver(values, index, remaining, cap), cap.reject)
    index++
  }
})

P.allSettled = combinator("allSettled", (rec, C, cap, promiseResolve) => {
  const values = []
  const remaining = { n: 1 }
  let index = 0
  for (;;) {
    const next = stepValue(rec)
    if (next === DONE) {
      if (--remaining.n === 0) {
        const resolve = cap.resolve
        resolve(values.slice())
      }
      return cap.promise
    }
    values.push(undefined)
    const nextPromise = promiseResolve.call(C, next)
    const flag = { called: false }
    remaining.n++
    nextPromise.then(
      settledResolver(values, index, remaining, cap, flag, false),
      settledResolver(values, index, remaining, cap, flag, true),
    )
    index++
  }
})

P.any = combinator("any", (rec, C, cap, promiseResolve) => {
  const errors = []
  const remaining = { n: 1 }
  let index = 0
  for (;;) {
    const next = stepValue(rec)
    if (next === DONE) {
      if (--remaining.n === 0) {
        const error = new AggregateError([], "All promises were rejected")
        Object.defineProperty(error, "errors", { value: errors.slice(), writable: true, enumerable: false, configurable: true })
        throw error
      }
      return cap.promise
    }
    errors.push(undefined)
    const nextPromise = promiseResolve.call(C, next)
    remaining.n++
    nextPromise.then(cap.resolve, anyRejecter(errors, index, remaining, cap))
    index++
  }
})

P.race = combinator("race", (rec, C, cap, promiseResolve) => {
  for (;;) {
    const next = stepValue(rec)
    if (next === DONE) return cap.promise
    const nextPromise = promiseResolve.call(C, next)
    nextPromise.then(cap.resolve, cap.reject)
  }
})

const statics = {
  resolve(x) {
    const C = this
    if (!isObject(C)) throw new TypeError("PromiseResolve called on non-object")
    return resolveWith(C, x)
  },
  reject(r) {
    const C = this
    const cap = newCapability(C)
    const reject = cap.reject
    reject(r)
    return cap.promise
  },
  withResolvers() {
    const cap = newCapability(this)
    return { promise: cap.promise, resolve: cap.resolve, reject: cap.reject }
  },
  try(fn, ...args) {
    const C = this
    if (!isObject(C)) throw new TypeError("Promise.try called on non-object")
    const cap = newCapability(C)
    let status
    try {
      status = fn(...args)
    } catch (e) {
      const reject = cap.reject
      reject(e)
      return cap.promise
    }
    const resolve = cap.resolve
    resolve(status)
    return cap.promise
  },
}
for (const name of Object.keys(statics)) {
  Object.defineProperty(P, name, { value: statics[name], writable: true, enumerable: false, configurable: true })
}

const protoMethods = {
  catch(onRejected) {
    return this.then(undefined, onRejected)
  },
  finally(onFinally) {
    const promise = this
    if (!isObject(promise)) throw new TypeError("Promise.prototype.finally called on non-object")
    const C = speciesOf(promise)
    let thenFinally = onFinally
    let catchFinally = onFinally
    if (typeof onFinally === "function") {
      thenFinally = ((value) => {
        const result = onFinally()
        const p = resolveWith(C, result)
        return p.then(() => value)
      })
      catchFinally = ((reason) => {
        const result = onFinally()
        const p = resolveWith(C, result)
        return p.then(() => {
          throw reason
        })
      })
      Object.defineProperty(thenFinally, "name", { value: "", configurable: true })
      Object.defineProperty(catchFinally, "name", { value: "", configurable: true })
    }
    return promise.then(thenFinally, catchFinally)
  },
}
for (const name of Object.keys(protoMethods)) {
  Object.defineProperty(P.prototype, name, { value: protoMethods[name], writable: true, enumerable: false, configurable: true })
}

Object.defineProperty(globalThis, "%PromiseThenSpecies%", {
  value: (p, C, onFulfilled, onRejected) => {
    const cap = newCapability(C)
    const resolve = cap.resolve
    const reject = cap.reject
    thenRaw.call(
      p,
      (v) => {
        if (typeof onFulfilled !== "function") {
          resolve(v)
          return
        }
        let r
        try {
          r = onFulfilled(v)
        } catch (e) {
          reject(e)
          return
        }
        resolve(r)
      },
      (e) => {
        if (typeof onRejected !== "function") {
          reject(e)
          return
        }
        let r
        try {
          r = onRejected(e)
        } catch (x) {
          reject(x)
          return
        }
        resolve(r)
      },
    )
    return cap.promise
  },
  enumerable: false,
})

const AsyncIteratorProto = Object.getPrototypeOf(Object.getPrototypeOf((async function* () {}).prototype))
const SYNC = Symbol("sync iterator")
const iterResult = (value, done) => ({ value, done })
const closeSyncQuietly = (rec) => {
  try {
    const ret = rec.it.return
    if (ret !== undefined && ret !== null) ret.call(rec.it)
  } catch {}
}
const thenInto = (p, onFulfilled, onRejected, cap) => {
  const resolve = cap.resolve
  const reject = cap.reject
  thenRaw.call(
    p,
    (v) => {
      let r
      try {
        r = onFulfilled(v)
      } catch (e) {
        reject(e)
        return
      }
      resolve(r)
    },
    (e) => {
      if (onRejected === undefined) {
        reject(e)
        return
      }
      let r
      try {
        r = onRejected(e)
      } catch (x) {
        reject(x)
        return
      }
      resolve(r)
    },
  )
}
const fromSyncContinuation = (result, cap, rec, closeOnRejection) => {
  const reject = cap.reject
  let done
  let value
  let wrapper
  try {
    done = Boolean(result.done)
    value = result.value
  } catch (e) {
    reject(e)
    return cap.promise
  }
  try {
    wrapper = resolveWith(P, value)
  } catch (e) {
    if (!done && closeOnRejection) closeSyncQuietly(rec)
    reject(e)
    return cap.promise
  }
  const onRejected =
    done || !closeOnRejection
      ? undefined
      : (error) => {
          closeSyncQuietly(rec)
          throw error
        }
  thenInto(wrapper, (v) => iterResult(v, done), onRejected, cap)
  return cap.promise
}
const AsyncFromSyncIteratorPrototype = Object.create(AsyncIteratorProto)
for (const [name, fn] of Object.entries({
  next(...args) {
    const rec = this[SYNC]
    const cap = newCapability(P)
    let result
    try {
      result = args.length > 0 ? rec.next.call(rec.it, args[0]) : rec.next.call(rec.it)
      if (!isObject(result)) throw new TypeError("Iterator result is not an object")
    } catch (e) {
      const reject = cap.reject
      reject(e)
      return cap.promise
    }
    return fromSyncContinuation(result, cap, rec, true)
  },
  return(...args) {
    const rec = this[SYNC]
    const cap = newCapability(P)
    const reject = cap.reject
    let ret
    try {
      ret = rec.it.return
    } catch (e) {
      reject(e)
      return cap.promise
    }
    if (ret === undefined || ret === null) {
      const resolve = cap.resolve
      resolve(iterResult(args[0], true))
      return cap.promise
    }
    let result
    try {
      result = args.length > 0 ? ret.call(rec.it, args[0]) : ret.call(rec.it)
    } catch (e) {
      reject(e)
      return cap.promise
    }
    if (!isObject(result)) {
      reject(new TypeError("Iterator result is not an object"))
      return cap.promise
    }
    return fromSyncContinuation(result, cap, rec, false)
  },
  throw(...args) {
    const rec = this[SYNC]
    const cap = newCapability(P)
    const reject = cap.reject
    let throwMethod
    try {
      throwMethod = rec.it.throw
    } catch (e) {
      reject(e)
      return cap.promise
    }
    if (throwMethod === undefined || throwMethod === null) {
      try {
        const ret = rec.it.return
        if (ret !== undefined && ret !== null) {
          const r = ret.call(rec.it)
          if (!isObject(r)) throw new TypeError("Iterator result is not an object")
        }
      } catch (e) {
        reject(e)
        return cap.promise
      }
      reject(new TypeError("The iterator does not provide a 'throw' method"))
      return cap.promise
    }
    let result
    try {
      result = args.length > 0 ? throwMethod.call(rec.it, args[0]) : throwMethod.call(rec.it)
    } catch (e) {
      reject(e)
      return cap.promise
    }
    if (!isObject(result)) {
      reject(new TypeError("Iterator result is not an object"))
      return cap.promise
    }
    return fromSyncContinuation(result, cap, rec, true)
  },
})) {
  Object.defineProperty(AsyncFromSyncIteratorPrototype, name, { value: fn, writable: true, enumerable: false, configurable: true })
}
Object.defineProperty(globalThis, "%CreateAsyncFromSyncIterator%", {
  value: (it, next) => {
    const o = Object.create(AsyncFromSyncIteratorPrototype)
    Object.defineProperty(o, SYNC, { value: { it, next, done: false } })
    return o
  },
  enumerable: false,
})
"##;

pub fn install(vm: &mut Vm) {
    let pp = vm.realm.promise_proto.clone();
    method(vm, &pp, "then", 2, then);
    let promise = make_ctor(
        vm,
        "Promise",
        1,
        promise_ctor,
        Some(|| ObjKind::Promise(Box::new(RefCell::new(PromiseData::default())))),
        &pp,
    );
    vm.define_global("Promise", Value::Object(promise));
    vm.promise_resolve_fn = native_fn(vm, "", 2, resolve_fn);
    vm.promise_reject_fn = native_fn(vm, "", 2, reject_fn);
    let raw = native_fn(vm, "then", 2, then_raw);
    vm.define_global("%PromiseThenRaw%", raw);
    let f = native_fn(vm, "isConstructor", 1, is_constructor);
    vm.define_global("%IsConstructor%", f);
    let f = native_fn(vm, "isPromise", 1, is_promise);
    vm.define_global("%IsPromise%", f);
}

pub fn run_prelude(vm: &mut Vm) {
    let dir = std::env::current_dir().unwrap_or_default();
    let proto = vm
        .compile(PRELUDE, "<prelude>", dir, Mode::Module)
        .expect("prelude compiles");
    if let Err(e) = vm.run_proto(proto) {
        panic!("prelude failed: {}", inspect_plain(&e));
    }
    super::extras::run(vm);
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
    let default = vm.globals.get("Promise").cloned().unwrap_or_default();
    let c = super::typed::species_constructor(vm, &this, default.clone())?;
    if c.strict_eq(&default) {
        let derived = vm.promise_then(&p, arg(args, 0), arg(args, 1));
        return Ok(Value::Object(derived));
    }
    let helper = vm
        .globals
        .get("%PromiseThenSpecies%")
        .cloned()
        .unwrap_or_default();
    vm.call_value(
        &helper,
        Value::Undefined,
        &[this.clone(), c, arg(args, 0), arg(args, 1)],
    )
}

fn then_raw(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let p = this_promise(vm, &this)?;
    let derived = vm.promise_then(&p, arg(args, 0), arg(args, 1));
    Ok(Value::Object(derived))
}

fn is_constructor(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(vm.is_constructor(&arg(args, 0))))
}

fn is_promise(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(Vm::is_promise(&arg(args, 0))))
}

fn lock(record: &Value) -> bool {
    match record {
        Value::Object(r) => {
            if r.flags.get() & crate::object::NOT_EXTENSIBLE != 0 {
                return false;
            }
            r.flags.set(r.flags.get() | crate::object::NOT_EXTENSIBLE);
            true
        }
        _ => true,
    }
}

fn resolve_fn(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if let Value::Object(p) = &this {
        if lock(&arg(args, 0)) {
            vm.resolve_promise(p, arg(args, 1));
        }
    }
    Ok(Value::Undefined)
}

fn reject_fn(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if let Value::Object(p) = &this {
        if lock(&arg(args, 0)) {
            vm.reject_promise(p, arg(args, 1));
        }
    }
    Ok(Value::Undefined)
}
