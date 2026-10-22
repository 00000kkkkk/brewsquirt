use crate::compiler::Mode;
use crate::inspect::inspect_plain;
use crate::vm::Vm;

const EXTRAS: &str = r##"
const define = (obj, name, value) =>
  Object.defineProperty(obj, name, { value, writable: true, enumerable: false, configurable: true })
const defineAll = (obj, methods) => {
  for (const name of Reflect.ownKeys(methods)) define(obj, name, methods[name])
}
const named = (name, fn) => Object.defineProperty(fn, "name", { value: name, configurable: true })
const isCallable = (f) => typeof f === "function"
const toStringStrict = (v) => {
  if (typeof v === "symbol") throw new TypeError("Cannot convert a Symbol value to a string")
  return String(v)
}
const needFn = (f, what) => {
  if (!isCallable(f)) throw new TypeError(`${typeof f === "string" ? `"${f}"` : String(f)} is not a function`)
}

const IteratorPrototype = Object.getPrototypeOf(Object.getPrototypeOf([][Symbol.iterator]()))
class Stamp {
  constructor(o) {
    return o
  }
}
class HelperBrand extends Stamp {
  #state
  constructor(o, state) {
    super(o)
    this.#state = state
  }
  static state(o) {
    return isObj(o) && #state in o ? o.#state : undefined
  }
}
class WrapBrand extends Stamp {
  #rec
  constructor(o, rec) {
    super(o)
    this.#rec = rec
  }
  static rec(o) {
    return isObj(o) && #rec in o ? o.#rec : undefined
  }
}

function Iterator() {
  if (new.target === undefined || new.target === Iterator) {
    throw new TypeError("Abstract class Iterator not directly constructable")
  }
}
Object.defineProperty(Iterator, "prototype", { value: IteratorPrototype, writable: false, enumerable: false, configurable: false })

const isObj = (v) => v !== null && (typeof v === "object" || typeof v === "function")

const ignoringSetter = (home, key) => function (v) {
  if (!isObj(this)) throw new TypeError("setter called on non-object")
  if (this === home) throw new TypeError(`Cannot assign to read only property '${String(key)}' of object`)
  if (Object.getOwnPropertyDescriptor(this, key) === undefined) {
    Object.defineProperty(this, key, { value: v, writable: true, enumerable: true, configurable: true })
  } else {
    this[key] = v
  }
}
Object.defineProperty(IteratorPrototype, Symbol.toStringTag, {
  get: named("get [Symbol.toStringTag]", function () {
    return "Iterator"
  }),
  set: named("set [Symbol.toStringTag]", ignoringSetter(IteratorPrototype, Symbol.toStringTag)),
  enumerable: false,
  configurable: true,
})
Object.defineProperty(IteratorPrototype, "constructor", {
  get: named("get constructor", function () {
    return Iterator
  }),
  set: named("set constructor", ignoringSetter(IteratorPrototype, "constructor")),
  enumerable: false,
  configurable: true,
})

const requireObject = (O, name) => {
  if (!isObj(O)) throw new TypeError(`Iterator.prototype.${name} called on non-object`)
  return O
}
const direct = (it) => ({ it, next: it.next, done: false })
const closeNormal = (it) => {
  const ret = it.return
  if (ret === undefined || ret === null) return
  const r = ret.call(it)
  if (!isObj(r)) throw new TypeError("Iterator result is not an object")
}
const closeThrow = (it) => {
  try {
    const ret = it.return
    if (ret !== undefined && ret !== null) ret.call(it)
  } catch {}
}
const failClose = (O, error) => {
  closeThrow(O)
  throw error
}
const DONE = Symbol("done")
const stepValue = (rec) => {
  const result = rec.next.call(rec.it)
  if (!isObj(result)) throw new TypeError("Iterator result is not an object")
  if (result.done) {
    rec.done = true
    return DONE
  }
  return result.value
}
const guard = (rec, fn) => {
  try {
    return fn()
  } catch (e) {
    closeThrow(rec.it)
    throw e
  }
}

const HelperPrototype = Object.create(IteratorPrototype)
const helperState = (O) => {
  const state = HelperBrand.state(O)
  if (state === undefined) {
    throw new TypeError("Iterator helper method called on incompatible receiver")
  }
  return state
}
defineAll(HelperPrototype, {
  next() {
    const O = helperState(this)
    O.started = true
    return O.gen.next()
  },
  return() {
    const O = helperState(this)
    if (!O.started) {
      O.started = true
      O.gen.return()
      closeNormal(O.rec.it)
      return { value: undefined, done: true }
    }
    return O.gen.return()
  },
})
Object.defineProperty(HelperPrototype, Symbol.toStringTag, { value: "Iterator Helper", writable: false, enumerable: false, configurable: true })

const helper = (rec, body) => {
  const out = Object.create(HelperPrototype)
  new HelperBrand(out, { gen: body(), rec, started: false })
  return out
}
function* yieldGuarded(rec, value) {
  let ok = false
  try {
    yield value
    ok = true
  } finally {
    if (!ok) closeNormal(rec.it)
  }
}

const toLimit = (O, limit) => {
  let n
  try {
    n = Number(limit)
  } catch (e) {
    failClose(O, e)
  }
  if (Number.isNaN(n)) failClose(O, new RangeError(`${String(limit)} must be positive`))
  const i = n === Infinity || n === -Infinity ? n : Math.trunc(n)
  if (i < 0) failClose(O, new RangeError(`${String(limit)} must be positive`))
  if (i !== Infinity && i > Number.MAX_SAFE_INTEGER) failClose(O, new RangeError(`${String(limit)} is too large`))
  return i
}
const needCallable = (O, fn) => {
  if (typeof fn !== "function") failClose(O, new TypeError(`${typeof fn === "string" ? `"${fn}"` : String(fn)} is not a function`))
}
const sizeArg = (O, n, what) => {
  if (typeof n !== "number") failClose(O, new TypeError(`${what} must be a number`))
  if (!Number.isInteger(n)) failClose(O, new TypeError(`${what} must be an integer`))
  if (n < 1 || n > 2 ** 32 - 1) failClose(O, new RangeError(`${what} is out of range`))
  return n
}

const flattenable = (obj, strings) => {
  if (!isObj(obj)) {
    if (!(strings && typeof obj === "string")) throw new TypeError(`${String(obj)} is not an object`)
  }
  const method = obj[Symbol.iterator]
  let it
  if (method === undefined || method === null) {
    it = obj
  } else {
    if (typeof method !== "function") throw new TypeError(`${String(method)} is not a function`)
    it = method.call(obj)
  }
  if (!isObj(it)) throw new TypeError(`${String(it)} is not an object`)
  return direct(it)
}

const methods = {
  map(mapper) {
    const O = requireObject(this, "map")
    needCallable(O, mapper)
    const rec = direct(O)
    return helper(rec, function* () {
      let counter = 0
      for (;;) {
        const value = stepValue(rec)
        if (value === DONE) return undefined
        const c = counter++
        const mapped = guard(rec, () => mapper(value, c))
        yield* yieldGuarded(rec, mapped)
      }
    })
  },
  filter(predicate) {
    const O = requireObject(this, "filter")
    needCallable(O, predicate)
    const rec = direct(O)
    return helper(rec, function* () {
      let counter = 0
      for (;;) {
        const value = stepValue(rec)
        if (value === DONE) return undefined
        const c = counter++
        if (guard(rec, () => predicate(value, c))) yield* yieldGuarded(rec, value)
      }
    })
  },
  take(limit) {
    const O = requireObject(this, "take")
    const n = toLimit(O, limit)
    const rec = direct(O)
    return helper(rec, function* () {
      let remaining = n
      for (;;) {
        if (remaining === 0) {
          closeNormal(rec.it)
          return undefined
        }
        if (remaining !== Infinity) remaining--
        const value = stepValue(rec)
        if (value === DONE) return undefined
        yield* yieldGuarded(rec, value)
      }
    })
  },
  drop(limit) {
    const O = requireObject(this, "drop")
    const n = toLimit(O, limit)
    const rec = direct(O)
    return helper(rec, function* () {
      let remaining = n
      while (remaining > 0) {
        if (remaining !== Infinity) remaining--
        const value = stepValue(rec)
        if (value === DONE) return undefined
      }
      for (;;) {
        const value = stepValue(rec)
        if (value === DONE) return undefined
        yield* yieldGuarded(rec, value)
      }
    })
  },
  flatMap(mapper) {
    const O = requireObject(this, "flatMap")
    needCallable(O, mapper)
    const rec = direct(O)
    return helper(rec, function* () {
      let counter = 0
      for (;;) {
        const value = stepValue(rec)
        if (value === DONE) return undefined
        const c = counter++
        const inner = guard(rec, () => flattenable(mapper(value, c), false))
        for (;;) {
          const v = guard(rec, () => stepValue(inner))
          if (v === DONE) break
          let ok = false
          try {
            yield v
            ok = true
          } finally {
            if (!ok) {
              try {
                closeNormal(inner.it)
              } catch (e) {
                closeThrow(rec.it)
                throw e
              }
              closeNormal(rec.it)
            }
          }
        }
      }
    })
  },
  reduce(reducer, ...rest) {
    const O = requireObject(this, "reduce")
    needCallable(O, reducer)
    const rec = direct(O)
    let acc
    let counter
    if (rest.length === 0) {
      acc = stepValue(rec)
      if (acc === DONE) throw new TypeError("Reduce of empty iterator with no initial value")
      counter = 1
    } else {
      acc = rest[0]
      counter = 0
    }
    for (;;) {
      const value = stepValue(rec)
      if (value === DONE) return acc
      const c = counter++
      const a = acc
      acc = guard(rec, () => reducer(a, value, c))
    }
  },
  toArray() {
    const O = requireObject(this, "toArray")
    const rec = direct(O)
    const items = []
    for (;;) {
      const value = stepValue(rec)
      if (value === DONE) return items
      items.push(value)
    }
  },
  forEach(fn) {
    const O = requireObject(this, "forEach")
    needCallable(O, fn)
    const rec = direct(O)
    let counter = 0
    for (;;) {
      const value = stepValue(rec)
      if (value === DONE) return undefined
      const c = counter++
      guard(rec, () => fn(value, c))
    }
  },
  some(predicate) {
    const O = requireObject(this, "some")
    needCallable(O, predicate)
    const rec = direct(O)
    let counter = 0
    for (;;) {
      const value = stepValue(rec)
      if (value === DONE) return false
      const c = counter++
      if (guard(rec, () => predicate(value, c))) {
        closeNormal(rec.it)
        return true
      }
    }
  },
  every(predicate) {
    const O = requireObject(this, "every")
    needCallable(O, predicate)
    const rec = direct(O)
    let counter = 0
    for (;;) {
      const value = stepValue(rec)
      if (value === DONE) return true
      const c = counter++
      if (!guard(rec, () => predicate(value, c))) {
        closeNormal(rec.it)
        return false
      }
    }
  },
  find(predicate) {
    const O = requireObject(this, "find")
    needCallable(O, predicate)
    const rec = direct(O)
    let counter = 0
    for (;;) {
      const value = stepValue(rec)
      if (value === DONE) return undefined
      const c = counter++
      if (guard(rec, () => predicate(value, c))) {
        closeNormal(rec.it)
        return value
      }
    }
  },
  includes(searchElement, skippedElements) {
    const O = requireObject(this, "includes")
    let toSkip = 0
    if (skippedElements !== undefined) {
      if (typeof skippedElements !== "number") failClose(O, new TypeError("skippedElements must be a number"))
      if (Number.isNaN(skippedElements)) failClose(O, new TypeError("skippedElements must not be NaN"))
      if (skippedElements === -Infinity) failClose(O, new RangeError("skippedElements must not be negative"))
      if (skippedElements !== Infinity) {
        if (!Number.isInteger(skippedElements)) failClose(O, new TypeError("skippedElements must be an integer"))
        if (skippedElements < 0) failClose(O, new RangeError("skippedElements must not be negative"))
        if (skippedElements > Number.MAX_SAFE_INTEGER) failClose(O, new RangeError("skippedElements is too large"))
      }
      toSkip = skippedElements
    }
    const rec = direct(O)
    let skipped = 0
    for (;;) {
      const value = stepValue(rec)
      if (value === DONE) return false
      if (skipped < toSkip) {
        skipped++
        continue
      }
      if (value === searchElement || (value !== value && searchElement !== searchElement)) {
        closeNormal(rec.it)
        return true
      }
    }
  },
  chunks(chunkSize) {
    const O = requireObject(this, "chunks")
    const size = sizeArg(O, chunkSize, "chunkSize")
    const rec = direct(O)
    return helper(rec, function* () {
      let buffer = []
      for (;;) {
        const value = stepValue(rec)
        if (value === DONE) {
          if (buffer.length > 0) yield* yieldGuarded(rec, buffer)
          return undefined
        }
        buffer.push(value)
        if (buffer.length === size) {
          const out = buffer
          buffer = []
          yield* yieldGuarded(rec, out)
        }
      }
    })
  },
  windows(windowSize, undersized) {
    const O = requireObject(this, "windows")
    const size = sizeArg(O, windowSize, "windowSize")
    if (undersized === undefined) undersized = "only-full"
    if (undersized !== "only-full" && undersized !== "allow-partial") {
      failClose(O, new TypeError('undersized must be "only-full" or "allow-partial"'))
    }
    const rec = direct(O)
    return helper(rec, function* () {
      const buffer = []
      let emitted = false
      for (;;) {
        const value = stepValue(rec)
        if (value === DONE) {
          if (undersized === "allow-partial" && !emitted && buffer.length > 0) yield* yieldGuarded(rec, buffer.slice())
          return undefined
        }
        if (buffer.length === size) buffer.shift()
        buffer.push(value)
        if (buffer.length === size) {
          emitted = true
          yield* yieldGuarded(rec, buffer.slice())
        }
      }
    })
  },
  join(separator) {
    const O = requireObject(this, "join")
    let sep = ","
    if (separator !== undefined) {
      try {
        sep = String(separator)
      } catch (e) {
        failClose(O, e)
      }
    }
    const rec = direct(O)
    let out = ""
    let first = true
    for (;;) {
      const value = stepValue(rec)
      if (value === DONE) return out
      if (!first) out += sep
      first = false
      if (value !== undefined && value !== null) out += guard(rec, () => String(value))
    }
  },
}
for (const name of Object.keys(methods)) {
  define(IteratorPrototype, name, methods[name])
}
Object.defineProperty(IteratorPrototype.includes, "length", { value: 1, configurable: true })
Object.defineProperty(IteratorPrototype.windows, "length", { value: 1, configurable: true })

const WrapPrototype = Object.create(IteratorPrototype)
const wrapState = (O) => {
  const rec = WrapBrand.rec(O)
  if (rec === undefined) {
    throw new TypeError("Iterator wrapper method called on incompatible receiver")
  }
  return rec
}
defineAll(WrapPrototype, {
  next() {
    const rec = wrapState(this)
    return rec.next.call(rec.it)
  },
  return() {
    const rec = wrapState(this)
    const ret = rec.it.return
    if (ret === undefined || ret === null) return { value: undefined, done: true }
    return ret.call(rec.it)
  },
})

define(Iterator, "from", {
  from(O) {
    const rec = flattenable(O, true)
    if (rec.it instanceof Iterator) return rec.it
    const wrapper = Object.create(WrapPrototype)
    new WrapBrand(wrapper, rec)
    return wrapper
  },
}.from)
define(globalThis, "Iterator", Iterator)

const setHas = Set.prototype.has
const setAdd = Set.prototype.add
const setDelete = Set.prototype.delete
const setKeys = Set.prototype.keys
const requireSet = (s, name) => {
  try {
    setHas.call(s)
  } catch (e) {
    throw new TypeError(`Method Set.prototype.${name} called on incompatible receiver`)
  }
}
const setRecord = (other) => {
  if (other === null || (typeof other !== "object" && typeof other !== "function")) {
    throw new TypeError("The 'other' argument must be an object")
  }
  const numSize = +other.size
  if (Number.isNaN(numSize)) throw new TypeError("The 'size' property must be a number")
  const size = Math.trunc(numSize)
  if (size < 0) throw new RangeError("The 'size' property must be non-negative")
  const has = other.has
  needFn(has)
  const keys = other.keys
  needFn(keys)
  return { obj: other, size, has: (v) => has.call(other, v), keys: () => keys.call(other) }
}
const setCopy = (s) => {
  const out = new Set()
  for (const v of setKeys.call(s)) setAdd.call(out, v)
  return out
}
const otherKeys = (r) => ({ [Symbol.iterator]: () => r.keys() })

defineAll(Set.prototype, {
  union(other) {
    requireSet(this, "union")
    const r = setRecord(other)
    const out = setCopy(this)
    for (const v of otherKeys(r)) setAdd.call(out, v)
    return out
  },
  intersection(other) {
    requireSet(this, "intersection")
    const r = setRecord(other)
    const out = new Set()
    if (this.size <= r.size) {
      for (const v of setKeys.call(this)) if (r.has(v)) setAdd.call(out, v)
    } else {
      for (const v of otherKeys(r)) if (setHas.call(this, v)) setAdd.call(out, v)
    }
    return out
  },
  difference(other) {
    requireSet(this, "difference")
    const r = setRecord(other)
    const out = setCopy(this)
    if (this.size <= r.size) {
      for (const v of setKeys.call(this)) if (r.has(v)) setDelete.call(out, v)
    } else {
      for (const v of otherKeys(r)) setDelete.call(out, v)
    }
    return out
  },
  symmetricDifference(other) {
    requireSet(this, "symmetricDifference")
    const r = setRecord(other)
    const out = setCopy(this)
    for (const v of otherKeys(r)) {
      if (setHas.call(this, v)) setDelete.call(out, v)
      else setAdd.call(out, v)
    }
    return out
  },
  isSubsetOf(other) {
    requireSet(this, "isSubsetOf")
    const r = setRecord(other)
    if (this.size > r.size) return false
    for (const v of setKeys.call(this)) if (!r.has(v)) return false
    return true
  },
  isSupersetOf(other) {
    requireSet(this, "isSupersetOf")
    const r = setRecord(other)
    if (this.size < r.size) return false
    for (const v of otherKeys(r)) if (!setHas.call(this, v)) return false
    return true
  },
  isDisjointFrom(other) {
    requireSet(this, "isDisjointFrom")
    const r = setRecord(other)
    if (this.size <= r.size) {
      for (const v of setKeys.call(this)) if (r.has(v)) return false
    } else {
      for (const v of otherKeys(r)) if (setHas.call(this, v)) return false
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

{
  const symAsyncIterator = Symbol.asyncIterator
  const symIterator = Symbol.iterator
  const isConstructor = (C) => {
    if (typeof C !== "function") return false
    try {
      Reflect.construct(function () {}, [], C)
      return true
    } catch {
      return false
    }
  }
  const getMethod = (v, key) => {
    const f = v[key]
    if (f === undefined || f === null) return undefined
    if (typeof f !== "function") throw new TypeError(`${String(key)} is not a function`)
    return f
  }
  const defineElement = (A, k, v) =>
    Object.defineProperty(A, k, { value: v, writable: true, enumerable: true, configurable: true })
  const toLength = (v) => {
    const n = Math.trunc(+v)
    if (!(n > 0)) return 0
    return Math.min(n, 2 ** 53 - 1)
  }
  const fromAsync = {
    fromAsync(asyncItems, mapFn, thisArg) {
      const C = this
      return new Promise((resolve, reject) => {
        const run = async () => {
          const mapping = mapFn !== undefined
          if (mapping && typeof mapFn !== "function") throw new TypeError("Array.fromAsync: mapper is not a function")
          const usingAsync = getMethod(asyncItems, symAsyncIterator)
          const usingSync = usingAsync === undefined ? getMethod(asyncItems, symIterator) : undefined
          if (usingAsync !== undefined || usingSync !== undefined) {
            const A = isConstructor(C) ? new C() : []
            const sync = usingAsync === undefined
            const iterator = (sync ? usingSync : usingAsync).call(asyncItems)
            if (!isObj(iterator)) throw new TypeError("Result of the iterator method is not an object")
            const next = iterator.next
            const closeWith = async (error) => {
              try {
                const ret = iterator.return
                if (ret !== undefined && ret !== null) {
                  const r = ret.call(iterator)
                  if (!sync) await r
                }
              } catch {}
              throw error
            }
            let k = 0
            for (;;) {
              if (k >= 2 ** 53 - 1) await closeWith(new TypeError("Array.fromAsync: too many elements"))
              let result = next.call(iterator)
              if (!sync) result = await result
              if (!isObj(result)) throw new TypeError("Iterator result is not an object")
              if (result.done) {
                A.length = k
                return A
              }
              let value = result.value
              if (sync) {
                try {
                  value = await value
                } catch (e) {
                  await closeWith(e)
                }
              }
              if (mapping) {
                try {
                  value = mapFn.call(thisArg, value, k)
                  value = await value
                } catch (e) {
                  await closeWith(e)
                }
              }
              try {
                defineElement(A, k, value)
              } catch (e) {
                await closeWith(e)
              }
              k++
            }
          }
          const arrayLike = Object(asyncItems)
          const len = toLength(arrayLike.length)
          const A = isConstructor(C) ? new C(len) : new Array(len)
          for (let k = 0; k < len; k++) {
            let value = await arrayLike[k]
            if (mapping) value = await mapFn.call(thisArg, value, k)
            defineElement(A, k, value)
          }
          A.length = len
          return A
        }
        run().then(resolve, reject)
      })
    },
  }.fromAsync
  Object.defineProperty(fromAsync, "length", { value: 1, configurable: true })
  define(Array, "fromAsync", fromAsync)
}

Object.defineProperty(Array, Symbol.species, { get: Object.defineProperty(function () { return this }, "name", { value: "get [Symbol.species]" }), configurable: true })
{
  const unscopables = Object.create(null)
  for (const k of ["at", "copyWithin", "entries", "fill", "find", "findIndex", "findLast", "findLastIndex", "flat", "flatMap", "includes", "keys", "toReversed", "toSorted", "toSpliced", "values"]) {
    unscopables[k] = true
  }
  Object.defineProperty(Array.prototype, Symbol.unscopables, { value: unscopables, writable: false, enumerable: false, configurable: true })
}
for (const C of [Map, Set, Promise, RegExp, ArrayBuffer]) {
  Object.defineProperty(C, Symbol.species, { get: Object.defineProperty(function () { return this }, "name", { value: "get [Symbol.species]" }), configurable: true })
}

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
  const html = (self, value) => {
    if (self == null) throw new TypeError(`String.prototype.${name} called on null or undefined`)
    const S = String(self)
    const open = attr ? `<${tag} ${attr}="${String(value).replace(/"/g, "&quot;")}">` : `<${tag}>`
    return `${open}${S}</${tag}>`
  }
  const fn = attr
    ? { [name](value) { return html(this, value) } }[name]
    : { [name]() { return html(this) } }[name]
  define(String.prototype, name, fn)
}

const hex2 = (n) => n.toString(16).toUpperCase().padStart(2, "0")
const hex4 = (n) => n.toString(16).toUpperCase().padStart(4, "0")
const ESCAPE_KEEP = /[A-Za-z0-9@*_+\-./]/
define(globalThis, "escape", {
  escape(string) {
    const S = toStringStrict(string)
    let R = ""
    for (let i = 0; i < S.length; i++) {
      const c = S[i]
      if (ESCAPE_KEEP.test(c)) R += c
      else {
        const n = S.charCodeAt(i)
        R += n < 256 ? "%" + hex2(n) : "%u" + hex4(n)
      }
    }
    return R
  },
}.escape)
define(globalThis, "unescape", {
  unescape(string) {
    const S = toStringStrict(string)
    const isHex = (s) => /^[0-9A-Fa-f]+$/.test(s)
    let R = ""
    let k = 0
    while (k < S.length) {
      let c = S[k]
      if (c === "%") {
        if (k + 6 <= S.length && S[k + 1] === "u" && isHex(S.slice(k + 2, k + 6))) {
          c = String.fromCharCode(parseInt(S.slice(k + 2, k + 6), 16))
          k += 5
        } else if (k + 3 <= S.length && isHex(S.slice(k + 1, k + 3))) {
          c = String.fromCharCode(parseInt(S.slice(k + 1, k + 3), 16))
          k += 2
        }
      }
      R += c
      k++
    }
    return R
  },
}.unescape)

defineAll(Date.prototype, {
  getYear() {
    return this.getFullYear() - 1900
  },
  setYear(y) {
    const n = Number(y)
    if (n !== n) return this.setTime(NaN)
    const t = Math.trunc(n)
    return this.setFullYear(t >= 0 && t <= 99 ? 1900 + t : n)
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

const RegExpProto = RegExp.prototype
const builtinExec = RegExpProto.exec
const reLength = (v) => {
  const n = Math.trunc(Number(v)) || 0
  return n <= 0 ? 0 : Math.min(n, 2 ** 53 - 1)
}
const reInt = (v) => {
  const n = Number(v)
  if (Number.isNaN(n)) return 0
  return Math.trunc(n)
}
const requireObj = (R, name) => {
  if (!isObj(R)) throw new TypeError(`RegExp.prototype.${name} called on incompatible receiver ${String(R)}`)
  return R
}
const setLastIndex = (R, v) => {
  if (!Reflect.set(R, "lastIndex", v)) throw new TypeError("Cannot assign to read only property 'lastIndex' of object")
}
const toStr = (v) => {
  if (typeof v === "symbol") throw new TypeError("Cannot convert a Symbol value to a string")
  return String(v)
}
const $slice = String.prototype.slice
const $indexOf = String.prototype.indexOf
const $charCodeAt = String.prototype.charCodeAt
const $codePointAt = String.prototype.codePointAt
const $apply = Reflect.apply
const $define = Object.defineProperty
const defineAt = (A, i, v) => $define(A, i, { value: v, writable: true, enumerable: true, configurable: true })
const listPush = (list, v) => defineAt(list, list.length, v)
const strSlice = (s, a, b) => $apply($slice, s, [a, b])
const hasFlag = (flags, c) => $apply($indexOf, flags, [c]) !== -1
const regExpExec = (R, S) => {
  const exec = R.exec
  if (typeof exec === "function") {
    const result = exec.call(R, S)
    if (result !== null && !isObj(result)) throw new TypeError("exec result must be an object or null")
    return result
  }
  return builtinExec.call(R, S)
}
const advance = (S, index, unicode) => {
  if (!unicode || index + 1 >= S.length) return index + 1
  const c = $apply($codePointAt, S, [index])
  return index + (c > 0xffff ? 2 : 1)
}
const speciesOf = (O, D) => {
  const C = O.constructor
  if (C === undefined) return D
  if (!isObj(C)) throw new TypeError("object.constructor is not an object")
  const S = C[Symbol.species]
  if (S === undefined || S === null) return D
  if (typeof S === "function") return S
  throw new TypeError("object.constructor[Symbol.species] is not a constructor")
}
const getSubstitution = (matched, str, position, captures, namedCaptures, replacement) => {
  const m = captures.length
  const tailPos = position + matched.length
  let result = ""
  let i = 0
  while (i < replacement.length) {
    const c = replacement[i]
    if (c !== "$" || i + 1 >= replacement.length) {
      result += c
      i++
      continue
    }
    const n = replacement[i + 1]
    if (n === "$") {
      result += "$"
      i += 2
    } else if (n === "&") {
      result += matched
      i += 2
    } else if (n === "`") {
      result += strSlice(str, 0, position)
      i += 2
    } else if (n === "'") {
      if (tailPos < str.length) result += strSlice(str, tailPos)
      i += 2
    } else if (n >= "0" && n <= "9") {
      const d1 = $apply($charCodeAt, n, [0]) - 48
      const n2 = replacement[i + 2]
      const hasTwo = n2 !== undefined && n2 >= "0" && n2 <= "9"
      const two = hasTwo ? d1 * 10 + ($apply($charCodeAt, n2, [0]) - 48) : -1
      if (hasTwo && two >= 1 && two <= m) {
        const cap = captures[two - 1]
        if (cap !== undefined) result += cap
        i += 3
      } else if (d1 >= 1 && d1 <= m) {
        const cap = captures[d1 - 1]
        if (cap !== undefined) result += cap
        i += 2
      } else {
        result += "$"
        i += 1
      }
    } else if (n === "<") {
      const close = $apply($indexOf, replacement, [">", i + 2])
      if (namedCaptures === undefined || close < 0) {
        result += "$<"
        i += 2
      } else {
        const groupName = strSlice(replacement, i + 2, close)
        const capture = namedCaptures[groupName]
        if (capture !== undefined) result += toStr(capture)
        i = close + 1
      }
    } else {
      result += "$"
      i += 1
    }
  }
  return result
}

const RegExpStringIteratorPrototype = Object.create(IteratorPrototype, {
  next: {
    value: named("next", function () {
      const O = this
      if (!isObj(O) || !(STATE in O)) throw new TypeError("next method called on incompatible receiver")
      const st = O[STATE]
      if (st.done) return { value: undefined, done: true }
      const match = regExpExec(st.R, st.S)
      if (match === null) {
        st.done = true
        return { value: undefined, done: true }
      }
      if (!st.global) {
        st.done = true
        return { value: match, done: false }
      }
      const matchStr = toStr(match[0])
      if (matchStr === "") {
        const thisIndex = reLength(st.R.lastIndex)
        setLastIndex(st.R, advance(st.S, thisIndex, st.unicode))
      }
      return { value: match, done: false }
    }),
    writable: true,
    configurable: true,
  },
  [Symbol.toStringTag]: { value: "RegExp String Iterator", configurable: true },
})
const STATE = Symbol("regexp iterator state")

defineAll(RegExpProto, {
  test(S) {
    const R = requireObj(this, "test")
    return regExpExec(R, toStr(S)) !== null
  },
  [Symbol.match](string) {
    const rx = requireObj(this, "[Symbol.match]")
    const S = toStr(string)
    const flags = toStr(rx.flags)
    if (!hasFlag(flags, "g")) return regExpExec(rx, S)
    const fullUnicode = hasFlag(flags, "u") || hasFlag(flags, "v")
    setLastIndex(rx, 0)
    const A = []
    let n = 0
    for (;;) {
      const result = regExpExec(rx, S)
      if (result === null) return n === 0 ? null : A
      const matchStr = toStr(result[0])
      defineAt(A, n++, matchStr)
      if (matchStr === "") {
        const thisIndex = reLength(rx.lastIndex)
        setLastIndex(rx, advance(S, thisIndex, fullUnicode))
      }
    }
  },
  [Symbol.matchAll](string) {
    const R = requireObj(this, "[Symbol.matchAll]")
    const S = toStr(string)
    const C = speciesOf(R, RegExp)
    const flags = toStr(R.flags)
    const matcher = new C(R, flags)
    setLastIndex(matcher, reLength(R.lastIndex))
    const it = Object.create(RegExpStringIteratorPrototype)
    Object.defineProperty(it, STATE, {
      value: { R: matcher, S, global: hasFlag(flags, "g"), unicode: hasFlag(flags, "u") || hasFlag(flags, "v"), done: false },
    })
    return it
  },
  [Symbol.replace](string, replaceValue) {
    const rx = requireObj(this, "[Symbol.replace]")
    const S = toStr(string)
    const lengthS = S.length
    const functionalReplace = typeof replaceValue === "function"
    if (!functionalReplace) replaceValue = toStr(replaceValue)
    const flags = toStr(rx.flags)
    const global = hasFlag(flags, "g")
    let fullUnicode = false
    if (global) {
      fullUnicode = hasFlag(flags, "u") || hasFlag(flags, "v")
      setLastIndex(rx, 0)
    }
    const results = []
    for (;;) {
      const result = regExpExec(rx, S)
      if (result === null) break
      listPush(results, result)
      if (!global) break
      const matchStr = toStr(result[0])
      if (matchStr === "") {
        const thisIndex = reLength(rx.lastIndex)
        setLastIndex(rx, advance(S, thisIndex, fullUnicode))
      }
    }
    let accumulated = ""
    let nextSourcePosition = 0
    for (const result of results) {
      const resultLength = reLength(result.length)
      const nCaptures = Math.max(resultLength - 1, 0)
      const matched = toStr(result[0])
      const matchLength = matched.length
      const position = Math.max(Math.min(reInt(result.index), lengthS), 0)
      const captures = []
      for (let n = 1; n <= nCaptures; n++) {
        let capN = result[n]
        if (capN !== undefined) capN = toStr(capN)
        listPush(captures, capN)
      }
      let namedCaptures = result.groups
      let replacement
      if (functionalReplace) {
        const args = [matched]
        for (let n = 0; n < captures.length; n++) listPush(args, captures[n])
        listPush(args, position)
        listPush(args, S)
        if (namedCaptures !== undefined) listPush(args, namedCaptures)
        replacement = toStr($apply(replaceValue, undefined, args))
      } else {
        if (namedCaptures !== undefined) {
          if (namedCaptures === null) throw new TypeError("Cannot convert undefined or null to object")
          namedCaptures = Object(namedCaptures)
        }
        replacement = getSubstitution(matched, S, position, captures, namedCaptures, replaceValue)
      }
      if (position >= nextSourcePosition) {
        accumulated += strSlice(S, nextSourcePosition, position) + replacement
        nextSourcePosition = position + matchLength
      }
    }
    if (nextSourcePosition >= lengthS) return accumulated
    return accumulated + strSlice(S, nextSourcePosition)
  },
  [Symbol.search](string) {
    const rx = requireObj(this, "[Symbol.search]")
    const S = toStr(string)
    const previousLastIndex = rx.lastIndex
    if (!Object.is(previousLastIndex, 0)) setLastIndex(rx, 0)
    const result = regExpExec(rx, S)
    const currentLastIndex = rx.lastIndex
    if (!Object.is(currentLastIndex, previousLastIndex)) setLastIndex(rx, previousLastIndex)
    return result === null ? -1 : result.index
  },
  [Symbol.split](string, limit) {
    const rx = requireObj(this, "[Symbol.split]")
    const S = toStr(string)
    const C = speciesOf(rx, RegExp)
    const flags = toStr(rx.flags)
    const unicodeMatching = hasFlag(flags, "u") || hasFlag(flags, "v")
    const newFlags = hasFlag(flags, "y") ? flags : flags + "y"
    const splitter = new C(rx, newFlags)
    const A = []
    let lengthA = 0
    const lim = limit === undefined ? 2 ** 32 - 1 : Number(limit) >>> 0
    if (lim === 0) return A
    const size = S.length
    if (size === 0) {
      const z = regExpExec(splitter, S)
      if (z !== null) return A
      defineAt(A, 0, S)
      return A
    }
    let p = 0
    let q = p
    while (q < size) {
      setLastIndex(splitter, q)
      const z = regExpExec(splitter, S)
      if (z === null) {
        q = advance(S, q, unicodeMatching)
        continue
      }
      const e = Math.min(reLength(splitter.lastIndex), size)
      if (e === p) {
        q = advance(S, q, unicodeMatching)
        continue
      }
      defineAt(A, lengthA++, strSlice(S, p, q))
      if (lengthA === lim) return A
      p = e
      const numberOfCaptures = Math.max(reLength(z.length) - 1, 0)
      for (let i = 1; i <= numberOfCaptures; i++) {
        defineAt(A, lengthA++, z[i])
        if (lengthA === lim) return A
      }
      q = p
    }
    defineAt(A, lengthA, strSlice(S, p, size))
    return A
  },
})
for (const [key, name] of [
  [Symbol.match, "[Symbol.match]"],
  [Symbol.matchAll, "[Symbol.matchAll]"],
  [Symbol.replace, "[Symbol.replace]"],
  [Symbol.search, "[Symbol.search]"],
  [Symbol.split, "[Symbol.split]"],
]) {
  named(name, RegExpProto[key])
}
Object.defineProperty(RegExpProto, "flags", {
  get: named("get flags", function () {
    const R = this
    if (!isObj(R)) throw new TypeError("RegExp.prototype.flags getter called on non-object")
    let result = ""
    if (R.hasIndices) result += "d"
    if (R.global) result += "g"
    if (R.ignoreCase) result += "i"
    if (R.multiline) result += "m"
    if (R.dotAll) result += "s"
    if (R.unicode) result += "u"
    if (R.unicodeSets) result += "v"
    if (R.sticky) result += "y"
    return result
  }),
  configurable: true,
})

class SuppressedError extends Error {
  constructor(error, suppressed, message) {
    super(message)
    Object.defineProperty(this, "error", { value: error, writable: true, configurable: true })
    Object.defineProperty(this, "suppressed", { value: suppressed, writable: true, configurable: true })
  }
}
define(SuppressedError.prototype, "name", "SuppressedError")
define(globalThis, "SuppressedError", SuppressedError)

define(globalThis, "%makeCallSite%", function makeCallSite(file, line, name, top, construct, isEval) {
  const site = {
    getThis: () => undefined,
    getTypeName: () => (top ? null : "Object"),
    getFunction: () => undefined,
    getFunctionName: () => name || null,
    getMethodName: () => null,
    getFileName: () => file,
    getLineNumber: () => line,
    getColumnNumber: () => 1,
    getEnclosingLineNumber: () => line,
    getEnclosingColumnNumber: () => 1,
    getEvalOrigin: () => undefined,
    getScriptNameOrSourceURL: () => file,
    getScriptHash: () => "",
    getPosition: () => 0,
    getPromiseIndex: () => null,
    isToplevel: () => top,
    isEval: () => isEval,
    isNative: () => false,
    isConstructor: () => construct,
    isAsync: () => false,
    isPromiseAll: () => false,
    toString: () => (name ? `${name} (${file}:${line}:1)` : `${file}:${line}:1`),
  }
  return site
})
define(globalThis, "global", globalThis)
{
  const registry = new Map()
  const listOf = (name) => {
    let l = registry.get(name)
    if (!l) registry.set(name, (l = []))
    return l
  }
  const check = (fn) => {
    if (typeof fn !== "function") throw new TypeError(`The "listener" argument must be of type function. Received ${fn === null ? "null" : typeof fn}`)
  }
  const add = (name, fn, prepend, once) => {
    check(fn)
    const entry = { fn, once }
    if (prepend) listOf(name).unshift(entry)
    else listOf(name).push(entry)
    return process
  }
  const remove = (name, fn) => {
    const l = registry.get(name)
    if (!l) return process
    const i = l.findIndex((e) => e.fn === fn)
    if (i >= 0) l.splice(i, 1)
    return process
  }
  let warned = false
  let capture = null
  let captureListener = null
  const signalNumbers = { SIGHUP: 1, SIGINT: 2, SIGQUIT: 3, SIGILL: 4, SIGTRAP: 5, SIGABRT: 6, SIGBUS: 7, SIGFPE: 8, SIGKILL: 9, SIGUSR1: 10, SIGSEGV: 11, SIGUSR2: 12, SIGPIPE: 13, SIGALRM: 14, SIGTERM: 15, SIGCHLD: 17, SIGCONT: 18, SIGSTOP: 19, SIGTSTP: 20, SIGWINCH: 28 }
  const hr = process.hrtime
  define(hr, "bigint", named("bigint", function bigint() {
    const [s, n] = hr()
    return BigInt(s) * 1000000000n + BigInt(n)
  }))
  process.features = { inspector: false, debug: false, uv: true, ipv6: true, tls_alpn: true, tls_sni: true, tls_ocsp: true, tls: true, cached_builtins: true, require_module: true, typescript: false }
  process.config = { variables: { node_shared: false, node_module_version: 127, napi_build_version: "9", v8_enable_i18n_support: 1 }, target_defaults: { cflags: [], default_configuration: "Release", defines: [], include_dirs: [], libraries: [] } }
  process.execArgv = []
  process.argv0 = process.argv[0]
  process.ppid = 0
  process.title = "jsq"
  process.debugPort = 9229
  process.sourceMapsEnabled = false
  process.allowedNodeEnvironmentFlags = new Set()
  process.report = {
    compact: false,
    directory: "",
    filename: "",
    reportOnFatalError: false,
    reportOnSignal: false,
    reportOnUncaughtException: false,
    signal: "SIGUSR2",
    getReport: () => ({ header: { nodejsVersion: process.version, platform: process.platform, arch: process.arch } }),
    writeReport: () => "",
  }
  defineAll(process, {
    umask() {
      return 0o22
    },
    kill(pid, signal = "SIGTERM") {
      if (pid === process.pid) {
        const name = typeof signal === "number" ? Object.keys(signalNumbers).find((k) => signalNumbers[k] === signal) ?? String(signal) : String(signal)
        if (signal === 0 || signal === "0") return true
        if (!process.emit(name, name)) process.exit(128 + (signalNumbers[name] ?? 15))
        return true
      }
      const e = new Error(`kill ESRCH`)
      e.code = "ESRCH"
      e.errno = -3
      e.syscall = "kill"
      throw e
    },
    cpuUsage(previous) {
      const user = Math.round(performance.now() * 1000)
      const out = { user, system: 0 }
      if (previous) {
        out.user -= previous.user
        out.system -= previous.system
      }
      return out
    },
    resourceUsage() {
      const mem = process.memoryUsage()
      return { userCPUTime: Math.round(performance.now() * 1000), systemCPUTime: 0, maxRSS: Math.round(mem.rss / 1024), sharedMemorySize: 0, unsharedDataSize: 0, unsharedStackSize: 0, minorPageFault: 0, majorPageFault: 0, swappedOut: 0, fsRead: 0, fsWrite: 0, ipcSent: 0, ipcReceived: 0, signalsCount: 0, voluntaryContextSwitches: 0, involuntaryContextSwitches: 0 }
    },
    abort() {
      process.exit(134)
    },
    binding(name) {
      const e = new Error(`No such module: ${name}`)
      e.code = "ERR_INVALID_MODULE"
      throw e
    },
    setSourceMapsEnabled(value) {
      process.sourceMapsEnabled = !!value
    },
    getActiveResourcesInfo() {
      return []
    },
    setUncaughtExceptionCaptureCallback(fn) {
      if (fn !== null && typeof fn !== "function") throw new TypeError('The "fn" argument must be of type function or null')
      if (fn !== null && capture !== null) {
        const e = new Error("`process.setupUncaughtExceptionCapture()` was called while a capture callback was already active")
        e.code = "ERR_UNCAUGHT_EXCEPTION_CAPTURE_ALREADY_SET"
        throw e
      }
      if (captureListener) process.off("uncaughtException", captureListener)
      capture = fn
      captureListener = fn === null ? null : (err) => fn(err)
      if (captureListener) process.on("uncaughtException", captureListener)
    },
    hasUncaughtExceptionCaptureCallback() {
      return capture !== null
    },
    on(name, fn) {
      return add(name, fn, false, false)
    },
    addListener(name, fn) {
      return add(name, fn, false, false)
    },
    prependListener(name, fn) {
      return add(name, fn, true, false)
    },
    once(name, fn) {
      return add(name, fn, false, true)
    },
    prependOnceListener(name, fn) {
      return add(name, fn, true, true)
    },
    off(name, fn) {
      return remove(name, fn)
    },
    removeListener(name, fn) {
      return remove(name, fn)
    },
    removeAllListeners(name) {
      if (name === undefined) registry.clear()
      else registry.delete(name)
      return process
    },
    emit(name, ...args) {
      const l = registry.get(name)
      if (!l || l.length === 0) return false
      for (const entry of l.slice()) {
        if (entry.once) remove(name, entry.fn)
        entry.fn.apply(process, args)
      }
      return true
    },
    listeners(name) {
      return (registry.get(name) ?? []).map((e) => e.fn)
    },
    rawListeners(name) {
      return (registry.get(name) ?? []).map((e) => e.fn)
    },
    listenerCount(name) {
      return registry.get(name)?.length ?? 0
    },
    eventNames() {
      return [...registry.keys()].filter((k) => registry.get(k).length > 0)
    },
    setMaxListeners() {
      return process
    },
    getMaxListeners() {
      return 10
    },
    emitWarning(warning, type, code, ctor) {
      let detail
      if (type !== null && typeof type === "object") {
        ;({ type, code, detail, ctor } = type)
      }
      let err = warning
      if (typeof warning === "string") {
        err = new Error(warning)
        err.name = typeof type === "string" ? type : "Warning"
      } else if (!(warning instanceof Error)) {
        throw new TypeError('The "warning" argument must be of type string or an instance of Error')
      }
      if (code !== undefined) err.code = code
      if (detail !== undefined) err.detail = detail
      if (err.name === "DeprecationWarning" && process.noDeprecation) return
      if (err.name === "DeprecationWarning" && process.throwDeprecation) throw err
      process.nextTick(() => {
        process.emit("warning", err)
        if (process.env.NODE_NO_WARNINGS === "1") return
        const prefix = err.code ? `[${err.code}] ` : ""
        console.error(`(jsq:${process.pid}) ${prefix}${err.name}: ${err.message}`)
        if (!warned) {
          warned = true
          console.error("(Set NODE_NO_WARNINGS=1 to silence warnings)")
        }
      })
    },
  })
}
define(globalThis, "setImmediate", named("setImmediate", function setImmediate(fn, ...args) {
  return setTimeout(fn, 0, ...args)
}))
define(globalThis, "clearImmediate", named("clearImmediate", function clearImmediate(handle) {
  clearTimeout(handle)
}))
define(globalThis, "reportError", named("reportError", function reportError(error) {
  console.error("Uncaught", error)
}))
define(globalThis, "navigator", {
  get hardwareConcurrency() {
    return process.availableParallelism()
  },
  get language() {
    return "en-US"
  },
  get languages() {
    return ["en-US"]
  },
  get platform() {
    return process.platform
  },
  get userAgent() {
    return `JavaSquirt/${process.version.slice(1)}`
  },
  get onLine() {
    return true
  },
})
{
  const entries = []
  const marks = new Map()
  const origin = Date.now() - performance.now()
  define(performance, "timeOrigin", origin)
  defineAll(performance, {
    markResourceTiming(timingInfo, requestedUrl, initiatorType) {
      const entry = { name: String(requestedUrl), entryType: "resource", startTime: performance.now(), duration: 0, initiatorType: String(initiatorType ?? "other") }
      entries.push(entry)
      return entry
    },
    mark(name, options) {
      const entry = { name: String(name), entryType: "mark", startTime: options?.startTime ?? performance.now(), duration: 0, detail: options?.detail ?? null }
      marks.set(entry.name, entry)
      entries.push(entry)
      return entry
    },
    measure(name, start, end) {
      let startTime = 0
      let endTime = performance.now()
      if (typeof start === "object" && start !== null) {
        if (start.start !== undefined) startTime = typeof start.start === "string" ? marks.get(start.start)?.startTime ?? 0 : start.start
        if (start.end !== undefined) endTime = typeof start.end === "string" ? marks.get(start.end)?.startTime ?? endTime : start.end
        if (start.duration !== undefined) endTime = startTime + start.duration
      } else {
        if (start !== undefined) startTime = marks.get(String(start))?.startTime ?? 0
        if (end !== undefined) endTime = marks.get(String(end))?.startTime ?? endTime
      }
      const entry = { name: String(name), entryType: "measure", startTime, duration: endTime - startTime, detail: null }
      entries.push(entry)
      return entry
    },
    clearMarks(name) {
      if (name === undefined) marks.clear()
      else marks.delete(String(name))
      const keep = entries.filter((e) => e.entryType !== "mark" || (name !== undefined && e.name !== String(name)))
      entries.length = 0
      entries.push(...keep)
    },
    clearMeasures(name) {
      const keep = entries.filter((e) => e.entryType !== "measure" || (name !== undefined && e.name !== String(name)))
      entries.length = 0
      entries.push(...keep)
    },
    getEntries() {
      return entries.slice()
    },
    getEntriesByName(name, type) {
      return entries.filter((e) => e.name === String(name) && (type === undefined || e.entryType === type))
    },
    getEntriesByType(type) {
      return entries.filter((e) => e.entryType === type)
    },
    toJSON() {
      return { timeOrigin: origin }
    },
  })
}

{
  const timers = new Map()
  const counts = new Map()
  const cell = (v) => {
    if (typeof v === "string") return `'${v}'`
    if (typeof v === "bigint") return `${v}n`
    if (typeof v === "symbol") return v.toString()
    if (typeof v === "function") return `[Function: ${v.name || "(anonymous)"}]`
    if (v === null || typeof v !== "object") return String(v)
    if (Array.isArray(v)) return `[ ${v.map(cell).join(", ")} ]`
    if (v instanceof Date) return v.toISOString()
    const keys = Object.keys(v)
    return keys.length === 0 ? "{}" : `{ ${keys.map((k) => `${k}: ${cell(v[k])}`).join(", ")} }`
  }
  const renderTable = (data, columns) => {
    if (data === null || typeof data !== "object") return null
    let rows
    let indexName = "(index)"
    let keyColumn = false
    if (data instanceof Map) {
      rows = [...data].map(([k, v], i) => [i, v, k])
      indexName = "(iteration index)"
      keyColumn = true
    } else if (data instanceof Set) {
      rows = [...data].map((v, i) => [i, v])
      indexName = "(iteration index)"
    } else if (Array.isArray(data)) {
      rows = data.map((v, i) => [i, v])
    } else {
      rows = Object.keys(data).map((k) => [k, data[k]])
    }
    const header = [indexName]
    if (keyColumn) header.push("Key")
    const props = []
    let hasValues = false
    for (const row of rows) {
      const v = row[1]
      if (v !== null && typeof v === "object" && !(v instanceof Date)) {
        for (const k of Object.keys(v)) if (!props.includes(k)) props.push(k)
      } else hasValues = true
    }
    const picked = Array.isArray(columns) ? columns.map(String) : props
    header.push(...picked)
    if (hasValues && !Array.isArray(columns)) header.push("Values")
    const body = rows.map((row) => {
      const out = [String(row[0])]
      if (keyColumn) out.push(cell(row[2]))
      const v = row[1]
      const isObj = v !== null && typeof v === "object" && !(v instanceof Date)
      for (const k of picked) out.push(isObj && Object.hasOwn(v, k) ? cell(v[k]) : "")
      if (hasValues && !Array.isArray(columns)) out.push(isObj ? "" : cell(v))
      return out
    })
    const widths = header.map((h, i) => Math.max(h.length, ...body.map((r) => r[i].length)))
    const line = (l, m, r) => l + widths.map((w) => "─".repeat(w + 2)).join(m) + r
    const render = (cells) => "│" + cells.map((c, i) => ` ${c.padEnd(widths[i])} `).join("│") + "│"
    const out = [line("┌", "┬", "┐"), render(header), line("├", "┼", "┤"), ...body.map(render), line("└", "┴", "┘")]
    return out.join("\n")
  }
  define(globalThis, "%renderTable%", renderTable)
  const table = (data, columns) => {
    const text = renderTable(data, columns)
    console.log(text === null ? data : text)
  }
  defineAll(console, {
    table,
    dir(value) {
      console.log(value)
    },
    dirxml(...args) {
      console.log(...args)
    },
    time(label = "default") {
      label = String(label)
      if (timers.has(label)) console.warn(`Warning: Label '${label}' already exists for console.time()`)
      else timers.set(label, performance.now())
    },
    timeLog(label = "default", ...rest) {
      label = String(label)
      if (!timers.has(label)) console.warn(`Warning: No such label '${label}' for console.timeLog()`)
      else console.log(`${label}: ${(performance.now() - timers.get(label)).toFixed(3)}ms`, ...rest)
    },
    timeEnd(label = "default") {
      label = String(label)
      if (!timers.has(label)) console.warn(`Warning: No such label '${label}' for console.timeEnd()`)
      else {
        console.log(`${label}: ${(performance.now() - timers.get(label)).toFixed(3)}ms`)
        timers.delete(label)
      }
    },
    count(label = "default") {
      label = String(label)
      const n = (counts.get(label) ?? 0) + 1
      counts.set(label, n)
      console.log(`${label}: ${n}`)
    },
    countReset(label = "default") {
      label = String(label)
      if (!counts.has(label)) console.warn(`Warning: Count for '${label}' does not exist`)
      else counts.set(label, 0)
    },
    trace(...args) {
      const stack = (new Error().stack ?? "").split("\n").slice(1).filter((l) => !l.includes("<squirt:")).join("\n")
      console.error(args.length ? "Trace:" : "Trace", ...args)
      if (stack) console.error(stack)
    },
  })
}
"##;

pub const ARRAY_GENERIC: &str = r##"
const objectToString = Object.prototype.toString
const MAX_LENGTH = 2 ** 53 - 1
const toObj = (v, name) => {
  if (v === null || v === undefined) throw new TypeError(`Array.prototype.${name} called on null or undefined`)
  return typeof v === "object" || typeof v === "function" ? v : Object(v)
}
const toLen = (v) => {
  const n = Number(v)
  if (!(n > 0)) return 0
  return Math.min(Math.trunc(n), MAX_LENGTH)
}
const toInt = (v) => {
  const n = Number(v)
  if (Number.isNaN(n) || n === 0) return 0
  if (!Number.isFinite(n)) return n
  return Math.trunc(n)
}
const relIdx = (v, len) => {
  const n = toInt(v)
  return n < 0 ? Math.max(len + n, 0) : Math.min(n, len)
}
const hasIdx = (O, k) => (typeof O === "string" ? k < O.length : String(k) in O)
const lenOf = (O) => toLen(O.length)
const callFn = (f, name) => {
  if (typeof f !== "function") throw new TypeError(`${f === undefined ? "undefined" : typeof f === "string" ? `"${f}"` : String(f)} is not a function`)
  return f
}
const setLen = (O, n) => {
  if (!Reflect.set(O, "length", n)) throw new TypeError("Cannot assign to read only property 'length' of object")
}
const put = (O, k, v) => {
  if (!Reflect.set(O, String(k), v)) throw new TypeError(`Cannot assign to read only property '${String(k)}' of object`)
}
const del = (O, k) => {
  if (!Reflect.deleteProperty(O, String(k))) throw new TypeError(`Cannot delete property '${String(k)}' of ${Object.prototype.toString.call(O)}`)
}
const createData = (A, k, v) => {
  if (!Reflect.defineProperty(A, String(k), { value: v, writable: true, enumerable: true, configurable: true })) {
    throw new TypeError(`Cannot add property ${String(k)}, object is not extensible`)
  }
}
const speciesCreate = (O, len) => {
  if (!Array.isArray(O)) {
    if (len > 2 ** 32 - 1) throw new RangeError("Invalid array length")
    return new Array(len)
  }
  let C = O.constructor
  if (C !== null && (typeof C === "object" || typeof C === "function")) {
    C = C[Symbol.species]
    if (C === null) C = undefined
  }
  if (C === undefined) {
    if (len > 2 ** 32 - 1) throw new RangeError("Invalid array length")
    return new Array(len)
  }
  if (typeof C !== "function") throw new TypeError("object.constructor[Symbol.species] is not a constructor")
  return new C(len)
}
const isConcatSpreadable = (O) => {
  if (O === null || (typeof O !== "object" && typeof O !== "function")) return false
  const s = O[Symbol.isConcatSpreadable]
  if (s !== undefined) return Boolean(s)
  return Array.isArray(O)
}
const sortCompare = (cmp) => (x, y) => {
  if (x === undefined && y === undefined) return 0
  if (x === undefined) return 1
  if (y === undefined) return -1
  if (cmp !== undefined) {
    const v = Number(cmp(x, y))
    return Number.isNaN(v) ? 0 : v
  }
  const xs = String(x)
  const ys = String(y)
  return xs < ys ? -1 : xs > ys ? 1 : 0
}
const mergeSort = (list, cmp) => {
  if (list.length <= 1) return list
  const mid = list.length >> 1
  const a = mergeSort(list.slice(0, mid), cmp)
  const b = mergeSort(list.slice(mid), cmp)
  const out = []
  let i = 0
  let j = 0
  while (i < a.length && j < b.length) out.push(cmp(a[i], b[j]) <= 0 ? a[i++] : b[j++])
  while (i < a.length) out.push(a[i++])
  while (j < b.length) out.push(b[j++])
  return out
}
const flatten = (target, source, sourceLen, start, depth, mapper, thisArg) => {
  let t = start
  for (let i = 0; i < sourceLen; i++) {
    if (!hasIdx(source, i)) continue
    let el = source[i]
    if (mapper) el = mapper.call(thisArg, el, i, source)
    if (depth > 0 && Array.isArray(el)) {
      t = flatten(target, el, lenOf(el), t, depth - 1)
    } else {
      if (t >= MAX_LENGTH) throw new TypeError("array too long")
      createData(target, t++, el)
    }
  }
  return t
}
function* iterate(O, kind) {
  let i = 0
  for (;;) {
    const len = ArrayBuffer.isView(O) ? O.length : lenOf(O)
    if (i >= len) return
    if (kind === "keys") yield i
    else if (kind === "values") yield O[i]
    else yield [i, O[i]]
    i++
  }
}

const isCtor = (C) => {
  if (typeof C !== "function") return false
  try {
    Reflect.construct(function () {}, [], C)
    return true
  } catch {
    return false
  }
}

const ArrayGeneric = {
  from(items, mapFn, thisArg) {
    const C = this
    if (mapFn !== undefined) callFn(mapFn)
    if (items === null || items === undefined) throw new TypeError(`${items} is not iterable`)
    const usingIterator = items[Symbol.iterator]
    if (usingIterator !== undefined && usingIterator !== null) {
      callFn(usingIterator)
      const A = isCtor(C) ? new C() : []
      let k = 0
      for (const v of { [Symbol.iterator]: () => usingIterator.call(items) }) {
        createData(A, k, mapFn ? mapFn.call(thisArg, v, k) : v)
        k++
      }
      setLen(A, k)
      return A
    }
    const O = Object(items)
    const len = lenOf(O)
    const A = isCtor(C) ? new C(len) : new Array(len)
    for (let k = 0; k < len; k++) createData(A, k, mapFn ? mapFn.call(thisArg, O[k], k) : O[k])
    setLen(A, len)
    return A
  },
  of(...items) {
    const C = this
    const A = isCtor(C) ? new C(items.length) : new Array(items.length)
    items.forEach((v, k) => createData(A, k, v))
    setLen(A, items.length)
    return A
  },
  at(index) {
    const O = toObj(this, "at")
    const len = lenOf(O)
    const k = toInt(index)
    const i = k >= 0 ? k : len + k
    return i < 0 || i >= len ? undefined : O[i]
  },
  concat(...items) {
    const O = toObj(this, "concat")
    const A = speciesCreate(O, 0)
    let n = 0
    for (const E of [O, ...items]) {
      if (isConcatSpreadable(E)) {
        const len = lenOf(E)
        if (n + len > MAX_LENGTH) throw new TypeError("array too long")
        for (let k = 0; k < len; k++, n++) if (hasIdx(E, k)) createData(A, n, E[k])
      } else {
        if (n >= MAX_LENGTH) throw new TypeError("array too long")
        createData(A, n++, E)
      }
    }
    setLen(A, n)
    return A
  },
  copyWithin(target, start, end) {
    const O = toObj(this, "copyWithin")
    const len = lenOf(O)
    let to = relIdx(target, len)
    let from = relIdx(start, len)
    const fin = end === undefined ? len : relIdx(end, len)
    let count = Math.min(fin - from, len - to)
    let dir = 1
    if (from < to && to < from + count) {
      dir = -1
      from += count - 1
      to += count - 1
    }
    while (count > 0) {
      if (hasIdx(O, from)) put(O, to, O[from])
      else del(O, to)
      from += dir
      to += dir
      count--
    }
    return O
  },
  entries() {
    return iterate(toObj(this, "entries"), "entries")
  },
  keys() {
    return iterate(toObj(this, "keys"), "keys")
  },
  values() {
    return iterate(toObj(this, "values"), "values")
  },
  every(fn, thisArg) {
    const O = toObj(this, "every")
    const len = lenOf(O)
    callFn(fn)
    for (let k = 0; k < len; k++) if (hasIdx(O, k) && !fn.call(thisArg, O[k], k, O)) return false
    return true
  },
  some(fn, thisArg) {
    const O = toObj(this, "some")
    const len = lenOf(O)
    callFn(fn)
    for (let k = 0; k < len; k++) if (hasIdx(O, k) && fn.call(thisArg, O[k], k, O)) return true
    return false
  },
  forEach(fn, thisArg) {
    const O = toObj(this, "forEach")
    const len = lenOf(O)
    callFn(fn)
    for (let k = 0; k < len; k++) if (hasIdx(O, k)) fn.call(thisArg, O[k], k, O)
    return undefined
  },
  map(fn, thisArg) {
    const O = toObj(this, "map")
    const len = lenOf(O)
    callFn(fn)
    const A = speciesCreate(O, len)
    for (let k = 0; k < len; k++) if (hasIdx(O, k)) createData(A, k, fn.call(thisArg, O[k], k, O))
    return A
  },
  filter(fn, thisArg) {
    const O = toObj(this, "filter")
    const len = lenOf(O)
    callFn(fn)
    const A = speciesCreate(O, 0)
    let to = 0
    for (let k = 0; k < len; k++) {
      if (!hasIdx(O, k)) continue
      const v = O[k]
      if (fn.call(thisArg, v, k, O)) createData(A, to++, v)
    }
    return A
  },
  fill(value, start, end) {
    const O = toObj(this, "fill")
    const len = lenOf(O)
    const k0 = relIdx(start, len)
    const fin = end === undefined ? len : relIdx(end, len)
    for (let k = k0; k < fin; k++) put(O, k, value)
    return O
  },
  find(fn, thisArg) {
    const O = toObj(this, "find")
    const len = lenOf(O)
    callFn(fn)
    for (let k = 0; k < len; k++) {
      const v = O[k]
      if (fn.call(thisArg, v, k, O)) return v
    }
    return undefined
  },
  findIndex(fn, thisArg) {
    const O = toObj(this, "findIndex")
    const len = lenOf(O)
    callFn(fn)
    for (let k = 0; k < len; k++) if (fn.call(thisArg, O[k], k, O)) return k
    return -1
  },
  findLast(fn, thisArg) {
    const O = toObj(this, "findLast")
    const len = lenOf(O)
    callFn(fn)
    for (let k = len - 1; k >= 0; k--) {
      const v = O[k]
      if (fn.call(thisArg, v, k, O)) return v
    }
    return undefined
  },
  findLastIndex(fn, thisArg) {
    const O = toObj(this, "findLastIndex")
    const len = lenOf(O)
    callFn(fn)
    for (let k = len - 1; k >= 0; k--) if (fn.call(thisArg, O[k], k, O)) return k
    return -1
  },
  flat(depth) {
    const O = toObj(this, "flat")
    const len = lenOf(O)
    let d = depth === undefined ? 1 : toInt(depth)
    if (d < 0) d = 0
    const A = speciesCreate(O, 0)
    flatten(A, O, len, 0, d)
    return A
  },
  flatMap(fn, thisArg) {
    const O = toObj(this, "flatMap")
    const len = lenOf(O)
    callFn(fn)
    const A = speciesCreate(O, 0)
    flatten(A, O, len, 0, 1, fn, thisArg)
    return A
  },
  includes(search, fromIndex) {
    const O = toObj(this, "includes")
    const len = lenOf(O)
    if (len === 0) return false
    let n = toInt(fromIndex)
    if (n === Infinity) return false
    if (n === -Infinity) n = 0
    let k = n >= 0 ? n : Math.max(len + n, 0)
    for (; k < len; k++) {
      const v = O[k]
      if (v === search || (v !== v && search !== search)) return true
    }
    return false
  },
  indexOf(search, fromIndex) {
    const O = toObj(this, "indexOf")
    const len = lenOf(O)
    if (len === 0) return -1
    let n = toInt(fromIndex)
    if (n === Infinity) return -1
    if (n === -Infinity) n = 0
    for (let k = n >= 0 ? n : Math.max(len + n, 0); k < len; k++) if (hasIdx(O, k) && O[k] === search) return k
    return -1
  },
  lastIndexOf(search, ...rest) {
    const O = toObj(this, "lastIndexOf")
    const len = lenOf(O)
    if (len === 0) return -1
    const n = rest.length > 0 ? toInt(rest[0]) : len - 1
    if (n === -Infinity) return -1
    for (let k = n >= 0 ? Math.min(n, len - 1) : len + n; k >= 0; k--) if (hasIdx(O, k) && O[k] === search) return k
    return -1
  },
  join(separator) {
    const O = toObj(this, "join")
    const len = lenOf(O)
    const sep = separator === undefined ? "," : String(separator)
    let out = ""
    for (let k = 0; k < len; k++) {
      if (k > 0) out += sep
      const v = O[k]
      if (v !== undefined && v !== null) out += String(v)
    }
    return out
  },
  toString() {
    const O = toObj(this, "toString")
    const f = O.join
    return typeof f === "function" ? f.call(O) : objectToString.call(O)
  },
  toLocaleString(...args) {
    const O = toObj(this, "toLocaleString")
    const len = lenOf(O)
    let out = ""
    for (let k = 0; k < len; k++) {
      if (k > 0) out += ","
      const v = O[k]
      if (v !== undefined && v !== null) out += String(v.toLocaleString(...args))
    }
    return out
  },
  pop() {
    const O = toObj(this, "pop")
    const len = lenOf(O)
    if (len === 0) {
      setLen(O, 0)
      return undefined
    }
    const v = O[len - 1]
    del(O, len - 1)
    setLen(O, len - 1)
    return v
  },
  push(...items) {
    const O = toObj(this, "push")
    const len = lenOf(O)
    if (len + items.length > MAX_LENGTH) throw new TypeError("Pushing items would exceed the maximum array length")
    let n = len
    for (const v of items) put(O, n++, v)
    setLen(O, n)
    return n
  },
  reduce(fn, ...init) {
    const O = toObj(this, "reduce")
    const len = lenOf(O)
    callFn(fn)
    let k = 0
    let acc
    if (init.length > 0) acc = init[0]
    else {
      while (k < len && !hasIdx(O, k)) k++
      if (k >= len) throw new TypeError("Reduce of empty array with no initial value")
      acc = O[k++]
    }
    for (; k < len; k++) if (hasIdx(O, k)) acc = fn(acc, O[k], k, O)
    return acc
  },
  reduceRight(fn, ...init) {
    const O = toObj(this, "reduceRight")
    const len = lenOf(O)
    callFn(fn)
    let k = len - 1
    let acc
    if (init.length > 0) acc = init[0]
    else {
      while (k >= 0 && !hasIdx(O, k)) k--
      if (k < 0) throw new TypeError("Reduce of empty array with no initial value")
      acc = O[k--]
    }
    for (; k >= 0; k--) if (hasIdx(O, k)) acc = fn(acc, O[k], k, O)
    return acc
  },
  reverse() {
    const O = toObj(this, "reverse")
    const len = lenOf(O)
    const mid = Math.floor(len / 2)
    for (let lower = 0; lower < mid; lower++) {
      const upper = len - lower - 1
      const lowerExists = hasIdx(O, lower)
      const upperExists = hasIdx(O, upper)
      const lv = lowerExists ? O[lower] : undefined
      const uv = upperExists ? O[upper] : undefined
      if (lowerExists && upperExists) {
        put(O, lower, uv)
        put(O, upper, lv)
      } else if (upperExists) {
        put(O, lower, uv)
        del(O, upper)
      } else if (lowerExists) {
        del(O, lower)
        put(O, upper, lv)
      }
    }
    return O
  },
  shift() {
    const O = toObj(this, "shift")
    const len = lenOf(O)
    if (len === 0) {
      setLen(O, 0)
      return undefined
    }
    const first = O[0]
    for (let k = 1; k < len; k++) {
      if (hasIdx(O, k)) put(O, k - 1, O[k])
      else del(O, k - 1)
    }
    del(O, len - 1)
    setLen(O, len - 1)
    return first
  },
  unshift(...items) {
    const O = toObj(this, "unshift")
    const len = lenOf(O)
    const n = items.length
    if (n > 0) {
      if (len + n > MAX_LENGTH) throw new TypeError("Unshift would exceed the maximum array length")
      for (let k = len; k > 0; k--) {
        if (hasIdx(O, k - 1)) put(O, k + n - 1, O[k - 1])
        else del(O, k + n - 1)
      }
      items.forEach((v, j) => put(O, j, v))
    }
    setLen(O, len + n)
    return len + n
  },
  slice(start, end) {
    const O = toObj(this, "slice")
    const len = lenOf(O)
    const k0 = relIdx(start, len)
    const fin = end === undefined ? len : relIdx(end, len)
    const count = Math.max(fin - k0, 0)
    const A = speciesCreate(O, count)
    let n = 0
    for (let k = k0; k < fin; k++, n++) if (hasIdx(O, k)) createData(A, n, O[k])
    setLen(A, n)
    return A
  },
  sort(cmp) {
    if (cmp !== undefined) callFn(cmp)
    const O = toObj(this, "sort")
    const len = lenOf(O)
    const items = []
    for (let k = 0; k < len; k++) if (hasIdx(O, k)) items.push(O[k])
    const sorted = mergeSort(items, sortCompare(cmp))
    let i = 0
    for (; i < sorted.length; i++) put(O, i, sorted[i])
    for (; i < len; i++) if (hasIdx(O, i)) del(O, i)
    return O
  },
  splice(start, deleteCount, ...items) {
    const O = toObj(this, "splice")
    const len = lenOf(O)
    const actualStart = relIdx(start, len)
    let dc
    if (arguments.length === 0) dc = 0
    else if (arguments.length === 1) dc = len - actualStart
    else dc = Math.min(Math.max(toInt(deleteCount), 0), len - actualStart)
    if (len + items.length - dc > MAX_LENGTH) throw new TypeError("array too long")
    const A = speciesCreate(O, dc)
    for (let k = 0; k < dc; k++) {
      const from = actualStart + k
      if (hasIdx(O, from)) createData(A, k, O[from])
    }
    setLen(A, dc)
    const ic = items.length
    if (ic < dc) {
      for (let k = actualStart; k < len - dc; k++) {
        const from = k + dc
        const to = k + ic
        if (hasIdx(O, from)) put(O, to, O[from])
        else del(O, to)
      }
      for (let k = len; k > len - dc + ic; k--) del(O, k - 1)
    } else if (ic > dc) {
      for (let k = len - dc; k > actualStart; k--) {
        const from = k + dc - 1
        const to = k + ic - 1
        if (hasIdx(O, from)) put(O, to, O[from])
        else del(O, to)
      }
    }
    items.forEach((v, j) => put(O, actualStart + j, v))
    setLen(O, len - dc + ic)
    return A
  },
  toReversed() {
    const O = toObj(this, "toReversed")
    const len = lenOf(O)
    if (len > 2 ** 32 - 1) throw new RangeError("Invalid array length")
    const A = new Array(len)
    for (let k = 0; k < len; k++) A[k] = O[len - k - 1]
    return A
  },
  toSorted(cmp) {
    if (cmp !== undefined) callFn(cmp)
    const O = toObj(this, "toSorted")
    const len = lenOf(O)
    if (len > 2 ** 32 - 1) throw new RangeError("Invalid array length")
    const items = []
    for (let k = 0; k < len; k++) items.push(O[k])
    return mergeSort(items, sortCompare(cmp))
  },
  toSpliced(start, skip, ...items) {
    const O = toObj(this, "toSpliced")
    const len = lenOf(O)
    const s = relIdx(start, len)
    const count = arguments.length === 0 ? 0 : arguments.length === 1 ? len - s : Math.min(Math.max(toInt(skip), 0), len - s)
    const newLen = len + items.length - count
    if (newLen > MAX_LENGTH) throw new TypeError("array too long")
    if (newLen > 2 ** 32 - 1) throw new RangeError("Invalid array length")
    const A = new Array(newLen)
    let i = 0
    let r = s + count
    for (; i < s; i++) A[i] = O[i]
    for (const v of items) A[i++] = v
    for (; i < newLen; i++, r++) A[i] = O[r]
    return A
  },
  with(index, value) {
    const O = toObj(this, "with")
    const len = lenOf(O)
    const rel = toInt(index)
    const actual = rel >= 0 ? rel : len + rel
    if (actual >= len || actual < 0) throw new RangeError("Invalid index : " + index)
    if (len > 2 ** 32 - 1) throw new RangeError("Invalid array length")
    const A = new Array(len)
    for (let k = 0; k < len; k++) A[k] = k === actual ? value : O[k]
    return A
  },
}
Object.defineProperty(globalThis, "%ArrayGeneric%", { value: ArrayGeneric, writable: false, enumerable: false, configurable: false })
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
