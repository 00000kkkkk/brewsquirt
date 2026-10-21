use crate::compiler::Mode;
use crate::inspect::inspect_plain;
use crate::vm::Vm;

const EXTRAS: &str = r##"
const define = (obj, name, value) =>
  Object.defineProperty(obj, name, { value, writable: true, enumerable: false, configurable: true })
const defineAll = (obj, methods) => {
  for (const [name, fn] of Object.entries(methods)) define(obj, name, fn)
}
const named = (name, fn) => Object.defineProperty(fn, "name", { value: name, configurable: true })
const isCallable = (f) => typeof f === "function"
const needFn = (f, what) => {
  if (!isCallable(f)) throw new TypeError(`${typeof f === "string" ? `"${f}"` : String(f)} is not a function`)
}

const IteratorPrototype = Object.getPrototypeOf(Object.getPrototypeOf([][Symbol.iterator]()))
const RECORD = Symbol("record")
const GEN = Symbol("generator")
const CLOSE = Symbol("close")

function Iterator() {
  if (new.target === undefined || new.target === Iterator) {
    throw new TypeError("Abstract class Iterator not directly constructable")
  }
}
Object.defineProperty(Iterator, "prototype", { value: IteratorPrototype, writable: false, enumerable: false, configurable: false })
define(IteratorPrototype, "constructor", Iterator)
Object.defineProperty(IteratorPrototype, Symbol.toStringTag, { value: "Iterator", writable: true, enumerable: false, configurable: true })

const direct = (it) => {
  if (it === null || (typeof it !== "object" && typeof it !== "function")) {
    throw new TypeError(`${String(it)} is not an object`)
  }
  return { it, next: it.next }
}

const close = (it) => {
  const ret = it.return
  if (ret != null) ret.call(it)
}

const WrapPrototype = Object.create(IteratorPrototype)
defineAll(WrapPrototype, {
  next() {
    return this[RECORD].next.call(this[RECORD].it)
  },
  return() {
    const ret = this[RECORD].it.return
    return ret == null ? { value: undefined, done: true } : ret.call(this[RECORD].it)
  },
})

const helper = (record, body) => {
  let done = false
  const gen = body()
  const out = Object.create(HelperPrototype)
  Object.defineProperty(out, GEN, { value: gen })
  Object.defineProperty(out, CLOSE, {
    value: () => {
      if (!done) {
        done = true
        close(record.it)
      }
    },
  })
  return out
}

const HelperPrototype = Object.create(IteratorPrototype)
defineAll(HelperPrototype, {
  next() {
    return this[GEN].next()
  },
  return() {
    this[CLOSE]()
    return this[GEN].return()
  },
})
Object.defineProperty(HelperPrototype, Symbol.toStringTag, { value: "Iterator Helper", configurable: true })

const toCount = (n, name) => {
  const v = Number(n)
  if (Number.isNaN(v)) throw new RangeError(`${String(n)} must be positive`)
  const i = Math.trunc(v)
  if (i < 0) throw new RangeError(`${String(n)} must be positive`)
  return i
}

defineAll(IteratorPrototype, {
  map(fn) {
    const r = direct(this)
    needFn(fn)
    return helper(r, function* () {
      let i = 0
      for (;;) {
        const s = r.next.call(r.it)
        if (s.done) return
        yield fn(s.value, i++)
      }
    })
  },
  filter(fn) {
    const r = direct(this)
    needFn(fn)
    return helper(r, function* () {
      let i = 0
      for (;;) {
        const s = r.next.call(r.it)
        if (s.done) return
        if (fn(s.value, i++)) yield s.value
      }
    })
  },
  take(limit) {
    const r = direct(this)
    const n = toCount(limit)
    return helper(r, function* () {
      let left = n
      for (;;) {
        if (left-- <= 0) {
          close(r.it)
          return
        }
        const s = r.next.call(r.it)
        if (s.done) return
        yield s.value
      }
    })
  },
  drop(limit) {
    const r = direct(this)
    const n = toCount(limit)
    return helper(r, function* () {
      let left = n
      for (;;) {
        const s = r.next.call(r.it)
        if (s.done) return
        if (left > 0) {
          left--
          continue
        }
        yield s.value
      }
    })
  },
  flatMap(fn) {
    const r = direct(this)
    needFn(fn)
    return helper(r, function* () {
      let i = 0
      for (;;) {
        const s = r.next.call(r.it)
        if (s.done) return
        const inner = fn(s.value, i++)
        if (inner === null || (typeof inner !== "object" && typeof inner !== "function")) {
          throw new TypeError("flatMap mapper must return an iterable or iterator")
        }
        yield* Iterator.from(inner)
      }
    })
  },
  reduce(fn, ...init) {
    const r = direct(this)
    needFn(fn)
    let acc
    let i = 0
    if (init.length > 0) acc = init[0]
    else {
      const s = r.next.call(r.it)
      if (s.done) throw new TypeError("Reduce of empty iterator with no initial value")
      acc = s.value
      i = 1
    }
    for (;;) {
      const s = r.next.call(r.it)
      if (s.done) return acc
      acc = fn(acc, s.value, i++)
    }
  },
  toArray() {
    const r = direct(this)
    const out = []
    for (;;) {
      const s = r.next.call(r.it)
      if (s.done) return out
      out.push(s.value)
    }
  },
  forEach(fn) {
    const r = direct(this)
    needFn(fn)
    let i = 0
    for (;;) {
      const s = r.next.call(r.it)
      if (s.done) return undefined
      fn(s.value, i++)
    }
  },
  some(fn) {
    const r = direct(this)
    needFn(fn)
    let i = 0
    for (;;) {
      const s = r.next.call(r.it)
      if (s.done) return false
      if (fn(s.value, i++)) {
        close(r.it)
        return true
      }
    }
  },
  every(fn) {
    const r = direct(this)
    needFn(fn)
    let i = 0
    for (;;) {
      const s = r.next.call(r.it)
      if (s.done) return true
      if (!fn(s.value, i++)) {
        close(r.it)
        return false
      }
    }
  },
  find(fn) {
    const r = direct(this)
    needFn(fn)
    let i = 0
    for (;;) {
      const s = r.next.call(r.it)
      if (s.done) return undefined
      if (fn(s.value, i++)) {
        close(r.it)
        return s.value
      }
    }
  },
})

define(Iterator, "from", named("from", function from(obj) {
  if (typeof obj === "string") return obj[Symbol.iterator]()
  if (obj === null || (typeof obj !== "object" && typeof obj !== "function")) {
    throw new TypeError(`${String(obj)} is not an object`)
  }
  const method = obj[Symbol.iterator]
  let it
  if (method != null) {
    needFn(method)
    it = method.call(obj)
  } else {
    it = obj
  }
  if (it instanceof Iterator) return it
  const wrapper = Object.create(WrapPrototype)
  Object.defineProperty(wrapper, RECORD, { value: direct(it) })
  return wrapper
}))
define(globalThis, "Iterator", Iterator)

const setRecord = (other) => {
  if (other === null || typeof other !== "object") throw new TypeError("The 'other' argument must be an object")
  const size = Number(other.size)
  if (Number.isNaN(size)) throw new TypeError("The 'size' property must be a number")
  const has = other.has
  needFn(has)
  const keys = other.keys
  needFn(keys)
  return { obj: other, size, has: (v) => has.call(other, v), keys: () => keys.call(other) }
}

defineAll(Set.prototype, {
  union(other) {
    const r = setRecord(other)
    const out = new Set(this)
    for (const v of { [Symbol.iterator]: () => r.keys() }) out.add(v)
    return out
  },
  intersection(other) {
    const r = setRecord(other)
    const out = new Set()
    if (this.size <= r.size) {
      for (const v of this) if (r.has(v)) out.add(v)
    } else {
      for (const v of { [Symbol.iterator]: () => r.keys() }) if (this.has(v)) out.add(v)
    }
    return out
  },
  difference(other) {
    const r = setRecord(other)
    const out = new Set(this)
    if (this.size <= r.size) {
      for (const v of this) if (r.has(v)) out.delete(v)
    } else {
      for (const v of { [Symbol.iterator]: () => r.keys() }) out.delete(v)
    }
    return out
  },
  symmetricDifference(other) {
    const r = setRecord(other)
    const out = new Set(this)
    for (const v of { [Symbol.iterator]: () => r.keys() }) {
      if (this.has(v)) out.delete(v)
      else out.add(v)
    }
    return out
  },
  isSubsetOf(other) {
    const r = setRecord(other)
    if (this.size > r.size) return false
    for (const v of this) if (!r.has(v)) return false
    return true
  },
  isSupersetOf(other) {
    const r = setRecord(other)
    if (this.size < r.size) return false
    for (const v of { [Symbol.iterator]: () => r.keys() }) if (!this.has(v)) return false
    return true
  },
  isDisjointFrom(other) {
    const r = setRecord(other)
    if (this.size <= r.size) {
      for (const v of this) if (r.has(v)) return false
    } else {
      for (const v of { [Symbol.iterator]: () => r.keys() }) if (this.has(v)) return false
    }
    return true
  },
})

define(Object, "groupBy", named("groupBy", function groupBy(items, fn) {
  needFn(fn)
  const out = Object.create(null)
  let i = 0
  for (const v of items) {
    const k = fn(v, i++)
    const key = typeof k === "symbol" ? k : String(k)
    ;(out[key] ??= []).push(v)
  }
  return out
}))

define(Map, "groupBy", named("groupBy", function groupBy(items, fn) {
  needFn(fn)
  const out = new Map()
  let i = 0
  for (const v of items) {
    let k = fn(v, i++)
    if (Object.is(k, -0)) k = 0
    const list = out.get(k)
    if (list) list.push(v)
    else out.set(k, [v])
  }
  return out
}))

define(Array, "fromAsync", named("fromAsync", async function fromAsync(items, mapFn, thisArg) {
  if (mapFn !== undefined) needFn(mapFn)
  const out = []
  let i = 0
  if (items != null && items[Symbol.asyncIterator] != null) {
    for await (const v of items) out.push(mapFn ? await mapFn.call(thisArg, v, i++) : v)
  } else if (items != null && items[Symbol.iterator] != null) {
    for (const p of items) {
      const v = await p
      out.push(mapFn ? await mapFn.call(thisArg, v, i++) : v)
    }
  } else {
    const len = Math.trunc(Number(Object(items).length)) || 0
    for (let j = 0; j < len; j++) {
      const v = await items[j]
      out.push(mapFn ? await mapFn.call(thisArg, v, j) : v)
    }
  }
  return out
}))

define(Promise, "try", named("try", function (fn, ...args) {
  return new Promise((resolve) => resolve(fn(...args)))
}))

const relIndex = (i, len) => {
  const n = Math.trunc(Number(i)) || 0
  return n < 0 ? Math.max(len + n, 0) : Math.min(n, len)
}

defineAll(Array.prototype, {
  toSpliced(start, skip, ...items) {
    const len = this.length
    const s = relIndex(start, len)
    const count = arguments.length === 0 ? 0 : arguments.length === 1 ? len - s : Math.min(Math.max(Math.trunc(Number(skip)) || 0, 0), len - s)
    const out = []
    for (let i = 0; i < s; i++) out.push(this[i])
    for (const v of items) out.push(v)
    for (let i = s + count; i < len; i++) out.push(this[i])
    return out
  },
  with(index, value) {
    const len = this.length
    const n = Math.trunc(Number(index)) || 0
    const i = n < 0 ? len + n : n
    if (i < 0 || i >= len) throw new RangeError("Invalid index : " + index)
    const out = []
    for (let j = 0; j < len; j++) out.push(j === i ? value : this[j])
    return out
  },
  copyWithin(target, start, end) {
    const len = this.length
    const to = relIndex(target, len)
    const from = relIndex(start, len)
    const fin = end === undefined ? len : relIndex(end, len)
    const count = Math.min(fin - from, len - to)
    if (count <= 0) return this
    const copy = this.slice(from, from + count)
    for (let i = 0; i < count; i++) this[to + i] = copy[i]
    return this
  },
  toLocaleString() {
    return this.map((v) => (v == null ? "" : v.toLocaleString())).join(",")
  },
})

defineAll(String.prototype, {
  trimLeft: String.prototype.trimStart,
  trimRight: String.prototype.trimEnd,
})
for (const [name, tag, attr] of [
  ["anchor", "a", "name"],
  ["big", "big"],
  ["blink", "blink"],
  ["bold", "b"],
  ["fixed", "tt"],
  ["fontcolor", "font", "color"],
  ["fontsize", "font", "size"],
  ["italics", "i"],
  ["link", "a", "href"],
  ["small", "small"],
  ["strike", "strike"],
  ["sub", "sub"],
  ["sup", "sup"],
]) {
  define(String.prototype, name, named(name, function (value) {
    if (this == null) throw new TypeError(`String.prototype.${name} called on null or undefined`)
    const open = attr ? `<${tag} ${attr}="${String(value).replace(/"/g, "&quot;")}">` : `<${tag}>`
    return `${open}${String(this)}</${tag}>`
  }))
}

defineAll(Date.prototype, {
  getYear() {
    return this.getFullYear() - 1900
  },
  setYear(y) {
    const n = Number(y)
    return this.setFullYear(n >= 0 && n <= 99 ? 1900 + Math.trunc(n) : n)
  },
})

define(RegExp, "escape", named("escape", function escape(s) {
  if (typeof s !== "string") throw new TypeError("RegExp.escape requires a string")
  let out = ""
  for (let i = 0; i < s.length; i++) {
    const c = s[i]
    if (i === 0 && /[0-9A-Za-z]/.test(c)) out += "\\x" + c.charCodeAt(0).toString(16).padStart(2, "0")
    else if ("^$\\.*+?()[]{}|/".includes(c)) out += "\\" + c
    else if (",-=<>#&!%:;@~'`\"".includes(c)) out += "\\x" + c.charCodeAt(0).toString(16).padStart(2, "0")
    else if (/\s/.test(c)) out += c === " " ? "\\x20" : c === "\t" ? "\\t" : c === "\n" ? "\\n" : c === "\r" ? "\\r" : c === "\v" ? "\\v" : c === "\f" ? "\\f" : "\\u" + c.charCodeAt(0).toString(16).padStart(4, "0")
    else out += c
  }
  return out
}))

class FinalizationRegistry {
  #cleanup
  #cells = []
  constructor(cleanup) {
    needFn(cleanup)
    this.#cleanup = cleanup
  }
  register(target, held, token) {
    if (target === null || (typeof target !== "object" && typeof target !== "function" && typeof target !== "symbol")) {
      throw new TypeError("FinalizationRegistry.prototype.register: invalid target")
    }
    if (Object.is(target, held)) throw new TypeError("FinalizationRegistry.prototype.register: target and holdings must not be same")
    this.#cells.push({ ref: new WeakRef(target), held, token })
  }
  unregister(token) {
    if (token === null || (typeof token !== "object" && typeof token !== "function" && typeof token !== "symbol")) {
      throw new TypeError("Invalid unregisterToken ('" + String(token) + "')")
    }
    const before = this.#cells.length
    this.#cells = this.#cells.filter((c) => c.token !== token)
    return this.#cells.length !== before
  }
  cleanupSome(fn) {
    const cb = fn ?? this.#cleanup
    const dead = this.#cells.filter((c) => c.ref.deref() === undefined)
    this.#cells = this.#cells.filter((c) => c.ref.deref() !== undefined)
    for (const c of dead) cb(c.held)
  }
}
Object.defineProperty(FinalizationRegistry.prototype, Symbol.toStringTag, { value: "FinalizationRegistry", configurable: true })
define(globalThis, "FinalizationRegistry", FinalizationRegistry)

class SuppressedError extends Error {
  constructor(error, suppressed, message) {
    super(message)
    Object.defineProperty(this, "error", { value: error, writable: true, configurable: true })
    Object.defineProperty(this, "suppressed", { value: suppressed, writable: true, configurable: true })
  }
}
define(SuppressedError.prototype, "name", "SuppressedError")
define(globalThis, "SuppressedError", SuppressedError)
"##;

pub fn run(vm: &mut Vm) {
    let dir = std::env::current_dir().unwrap_or_default();
    let proto = vm
        .compile(EXTRAS, "<prelude>", dir, Mode::Module)
        .expect("extras compile");
    if let Err(e) = vm.run_proto(proto) {
        panic!("extras failed: {}", inspect_plain(&e));
    }
}
