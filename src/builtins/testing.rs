use super::{arg, method};
use crate::compiler::Mode;
use crate::inspect::inspect_plain;
use crate::value::Value;
use crate::vm::Vm;

type R = Result<Value, Value>;

const TEST_SRC: &str = r#"
import { inspect } from "squirt:util"

const suites = []
const registry = []
let only = false

function fullName(name) {
  return [...suites, name].join(" > ")
}

function register(name, fn, mode) {
  if (typeof fn !== "function") throw new TypeError(`test "${name}" needs a function`)
  if (mode === "only") only = true
  registry.push({ name: fullName(name), fn, mode, hooks: suites.map((s) => s) })
}

const beforeHooks = []
const afterHooks = []

export function test(name, fn) {
  register(name, fn, "run")
}
test.skip = (name, fn) => register(name, fn ?? (() => {}), "skip")
test.only = (name, fn) => register(name, fn, "only")
test.todo = (name) => register(name, () => {}, "todo")
export const it = test

export function describe(name, fn) {
  suites.push(name)
  try {
    fn()
  } finally {
    suites.pop()
  }
}
describe.skip = (name, fn) => {
  suites.push(name)
  const saved = registry.length
  try {
    fn()
  } finally {
    suites.pop()
  }
  for (let i = saved; i < registry.length; i++) registry[i].mode = "skip"
}

export function beforeEach(fn) {
  beforeHooks.push(fn)
}
export function afterEach(fn) {
  afterHooks.push(fn)
}

export function isDeepEqual(a, b) {
  if (Object.is(a, b)) return true
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false
  if (Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false
  if (a instanceof Date) return a.getTime() === b.getTime()
  if (a instanceof RegExp) return String(a) === String(b)
  if (a instanceof Map) {
    if (a.size !== b.size) return false
    for (const [k, v] of a) if (!b.has(k) || !isDeepEqual(v, b.get(k))) return false
    return true
  }
  if (a instanceof Set) {
    if (a.size !== b.size) return false
    for (const v of a) if (!b.has(v)) return false
    return true
  }
  if (Array.isArray(a)) {
    if (a.length !== b.length) return false
    for (let i = 0; i < a.length; i++) if (!isDeepEqual(a[i], b[i])) return false
    return true
  }
  const ka = Object.keys(a)
  const kb = Object.keys(b)
  if (ka.length !== kb.length) return false
  for (const k of ka) if (!Object.hasOwn(b, k) || !isDeepEqual(a[k], b[k])) return false
  return true
}

export class AssertionError extends Error {
  constructor(message, expected, actual) {
    super(message)
    this.name = "AssertionError"
    this.expected = expected
    this.actual = actual
  }
}

function fail(message, expected, actual) {
  throw new AssertionError(message, expected, actual)
}

function describeThrown(fn) {
  try {
    fn()
  } catch (e) {
    return { threw: true, error: e }
  }
  return { threw: false }
}

function matchesError(error, expected) {
  if (expected === undefined) return true
  const message = error instanceof Error ? error.message : String(error)
  if (typeof expected === "string") return message.includes(expected)
  if (expected instanceof RegExp) return expected.test(message)
  if (typeof expected === "function") return error instanceof expected
  return isDeepEqual(error, expected)
}

function makeMatchers(actual, negate, prefix) {
  const check = (pass, text, expected) => {
    if (pass === negate) {
      fail(`expect(${inspect(actual)})${prefix}${negate ? ".not" : ""}.${text}`, expected, actual)
    }
  }
  return {
    toBe: (e) => check(Object.is(actual, e), `toBe(${inspect(e)})`, e),
    toEqual: (e) => check(isDeepEqual(actual, e), `toEqual(${inspect(e)})`, e),
    toStrictEqual: (e) => check(isDeepEqual(actual, e), `toStrictEqual(${inspect(e)})`, e),
    toBeTruthy: () => check(!!actual, "toBeTruthy()"),
    toBeFalsy: () => check(!actual, "toBeFalsy()"),
    toBeNull: () => check(actual === null, "toBeNull()"),
    toBeUndefined: () => check(actual === undefined, "toBeUndefined()"),
    toBeDefined: () => check(actual !== undefined, "toBeDefined()"),
    toBeNaN: () => check(Number.isNaN(actual), "toBeNaN()"),
    toBeGreaterThan: (n) => check(actual > n, `toBeGreaterThan(${inspect(n)})`, n),
    toBeGreaterThanOrEqual: (n) => check(actual >= n, `toBeGreaterThanOrEqual(${inspect(n)})`, n),
    toBeLessThan: (n) => check(actual < n, `toBeLessThan(${inspect(n)})`, n),
    toBeLessThanOrEqual: (n) => check(actual <= n, `toBeLessThanOrEqual(${inspect(n)})`, n),
    toBeCloseTo: (n, digits = 2) =>
      check(Math.abs(actual - n) < 10 ** -digits / 2, `toBeCloseTo(${inspect(n)}, ${digits})`, n),
    toBeInstanceOf: (cls) => check(actual instanceof cls, `toBeInstanceOf(${cls.name})`),
    toContain: (item) => {
      const has = typeof actual === "string" ? actual.includes(item) : [...actual].some((x) => Object.is(x, item))
      check(has, `toContain(${inspect(item)})`, item)
    },
    toContainEqual: (item) => check([...actual].some((x) => isDeepEqual(x, item)), `toContainEqual(${inspect(item)})`, item),
    toHaveLength: (n) => check(actual != null && actual.length === n, `toHaveLength(${n})`, n),
    toHaveProperty: (key, ...value) => {
      const path = Array.isArray(key) ? key : String(key).split(".")
      let cur = actual
      let found = true
      for (const k of path) {
        if (cur != null && k in Object(cur)) cur = cur[k]
        else {
          found = false
          break
        }
      }
      const pass = found && (value.length === 0 || isDeepEqual(cur, value[0]))
      check(pass, `toHaveProperty(${inspect(key)}${value.length ? ", " + inspect(value[0]) : ""})`, value[0])
    },
    toMatch: (pattern) => {
      const pass = typeof pattern === "string" ? String(actual).includes(pattern) : pattern.test(String(actual))
      check(pass, `toMatch(${inspect(pattern)})`, pattern)
    },
    toMatchObject: (shape) => {
      const pass = typeof actual === "object" && actual !== null &&
        Object.keys(shape).every((k) => isDeepEqual(actual[k], shape[k]) ||
          (typeof shape[k] === "object" && shape[k] !== null && makeMatchersPass(actual[k], shape[k])))
      check(pass, `toMatchObject(${inspect(shape)})`, shape)
    },
    toThrow: (expected) => {
      if (typeof actual !== "function") fail("toThrow() expects a function")
      const r = describeThrown(actual)
      const pass = r.threw && matchesError(r.error, expected)
      check(pass, `toThrow(${expected === undefined ? "" : inspect(expected)})`, expected)
    },
  }
}

function makeMatchersPass(actual, shape) {
  try {
    makeMatchers(actual, false, "").toMatchObject(shape)
    return true
  } catch {
    return false
  }
}

function asyncMatchers(promise, expectReject, negate) {
  const sync = makeMatchers(undefined, negate, expectReject ? ".rejects" : ".resolves")
  const out = {}
  for (const name of Object.keys(sync)) {
    out[name] = async (...args) => {
      let value
      let rejected = false
      try {
        value = await promise
      } catch (e) {
        value = e
        rejected = true
      }
      if (rejected !== expectReject) {
        fail(`expected promise to ${expectReject ? "reject" : "resolve"} but it ${rejected ? "rejected" : "resolved"} with ${inspect(value)}`)
      }
      if (expectReject && name === "toThrow") {
        const pass = matchesError(value, args[0])
        if (pass === negate) fail(`expect(promise).rejects${negate ? ".not" : ""}.toThrow(${args.length ? inspect(args[0]) : ""})`)
        return
      }
      makeMatchers(value, negate, expectReject ? ".rejects" : ".resolves")[name](...args)
    }
  }
  return out
}

export function expect(actual) {
  const m = makeMatchers(actual, false, "")
  m.not = makeMatchers(actual, true, "")
  m.resolves = asyncMatchers(actual, false, false)
  m.rejects = asyncMatchers(actual, true, false)
  m.resolves.not = asyncMatchers(actual, false, true)
  m.rejects.not = asyncMatchers(actual, true, true)
  return m
}

export function assert(value, message) {
  if (!value) fail(message ?? `assert(${inspect(value)})`)
}
assert.ok = assert
assert.equal = (a, b, message) => {
  if (!Object.is(a, b)) fail(message ?? `${inspect(a)} !== ${inspect(b)}`, b, a)
}
assert.notEqual = (a, b, message) => {
  if (Object.is(a, b)) fail(message ?? `${inspect(a)} === ${inspect(b)}`, b, a)
}
assert.deepEqual = (a, b, message) => {
  if (!isDeepEqual(a, b)) fail(message ?? `${inspect(a)} is not deeply equal to ${inspect(b)}`, b, a)
}
assert.throws = (fn, expected, message) => {
  const r = describeThrown(fn)
  if (!r.threw || !matchesError(r.error, expected)) fail(message ?? "expected function to throw", expected)
}
assert.rejects = async (promise, expected, message) => {
  try {
    await (typeof promise === "function" ? promise() : promise)
  } catch (e) {
    if (!matchesError(e, expected)) fail(message ?? `unexpected rejection ${inspect(e)}`, expected, e)
    return
  }
  fail(message ?? "expected promise to reject")
}
assert.fail = (message) => fail(message ?? "assert.fail()")

function explain(error) {
  if (error instanceof AssertionError) {
    const lines = [error.message]
    if ("expected" in error && error.expected !== undefined) lines.push(`expected: ${inspect(error.expected)}`)
    if ("actual" in error && error.actual !== undefined) lines.push(`received: ${inspect(error.actual)}`)
    return lines
  }
  if (error instanceof Error) return (error.stack ?? String(error)).split("\n")
  return [`thrown: ${inspect(error)}`]
}

export async function run(file, filter) {
  const results = { passed: 0, failed: 0, skipped: 0, todo: 0 }
  const out = (line) => console.log(line)
  out(file)
  for (const t of registry) {
    if (filter && !t.name.includes(filter)) continue
    const skip = t.mode === "skip" || (only && t.mode !== "only")
    if (t.mode === "todo") {
      results.todo++
      out(`  - ${t.name} (todo)`)
      continue
    }
    if (skip) {
      results.skipped++
      out(`  - ${t.name} (skipped)`)
      continue
    }
    const started = performance.now()
    try {
      for (const h of beforeHooks) await h()
      await t.fn()
      for (const h of afterHooks) await h()
      results.passed++
      out(`  ✓ ${t.name} (${Math.round(performance.now() - started)}ms)`)
    } catch (e) {
      results.failed++
      out(`  ✗ ${t.name}`)
      for (const line of explain(e)) out(`      ${line}`)
    }
  }
  if (registry.length === 0) out("  (no tests)")
  return results
}
"#;

pub fn util_module(vm: &mut Vm) -> Value {
    let o = vm.new_object();
    method(vm, &o, "inspect", 1, util_inspect);
    method(vm, &o, "format", 1, util_format);
    method(vm, &o, "isDeepStrictEqual", 2, util_deep_equal);
    o.set_own("default", Value::Object(o.clone()));
    Value::Object(o)
}

fn util_inspect(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(inspect_plain(&arg(args, 0)).into())
}

fn util_format(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(super::system::format_args(vm, args)?.into())
}

fn util_deep_equal(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let module = vm.import_builtin("test")?;
    let f = vm.get_prop(&module, "isDeepEqual")?;
    vm.call_value(&f, Value::Undefined, args)
}

pub fn test_module(vm: &mut Vm) -> Result<Value, Value> {
    let dir = std::env::current_dir().unwrap_or_default();
    let proto = match vm.compile(TEST_SRC, "<squirt:test>", dir, Mode::Module) {
        Ok(p) => p,
        Err(e) => return Err(vm.syntax_error_value(&e, "<squirt:test>")),
    };
    vm.run_proto(proto)
}
