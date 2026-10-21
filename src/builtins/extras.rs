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

Object.defineProperty(Array, Symbol.species, { get: Object.defineProperty(function () { return this }, "name", { value: "get [Symbol.species]" }), configurable: true })
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

const RegExpProto = RegExp.prototype
const builtinExec = RegExpProto.exec
const isObj = (v) => v !== null && (typeof v === "object" || typeof v === "function")
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
  const c = S.codePointAt(index)
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
      result += str.slice(0, position)
      i += 2
    } else if (n === "'") {
      if (tailPos < str.length) result += str.slice(tailPos)
      i += 2
    } else if (n >= "0" && n <= "9") {
      const d1 = n.charCodeAt(0) - 48
      const n2 = replacement[i + 2]
      const hasTwo = n2 !== undefined && n2 >= "0" && n2 <= "9"
      const two = hasTwo ? d1 * 10 + (n2.charCodeAt(0) - 48) : -1
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
      const close = replacement.indexOf(">", i + 2)
      if (namedCaptures === undefined || close < 0) {
        result += "$<"
        i += 2
      } else {
        const groupName = replacement.slice(i + 2, close)
        const capture = namedCaptures[groupName]
        if (capture !== undefined) result += String(capture)
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
      const matchStr = String(match[0])
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
    return regExpExec(R, String(S)) !== null
  },
  [Symbol.match](string) {
    const rx = requireObj(this, "[Symbol.match]")
    const S = String(string)
    const flags = String(rx.flags)
    if (!flags.includes("g")) return regExpExec(rx, S)
    const fullUnicode = flags.includes("u") || flags.includes("v")
    setLastIndex(rx, 0)
    const A = []
    let n = 0
    for (;;) {
      const result = regExpExec(rx, S)
      if (result === null) return n === 0 ? null : A
      const matchStr = String(result[0])
      A[n++] = matchStr
      if (matchStr === "") {
        const thisIndex = reLength(rx.lastIndex)
        setLastIndex(rx, advance(S, thisIndex, fullUnicode))
      }
    }
  },
  [Symbol.matchAll](string) {
    const R = requireObj(this, "[Symbol.matchAll]")
    const S = String(string)
    const C = speciesOf(R, RegExp)
    const flags = String(R.flags)
    const matcher = new C(R, flags)
    setLastIndex(matcher, reLength(R.lastIndex))
    const it = Object.create(RegExpStringIteratorPrototype)
    Object.defineProperty(it, STATE, {
      value: { R: matcher, S, global: flags.includes("g"), unicode: flags.includes("u") || flags.includes("v"), done: false },
    })
    return it
  },
  [Symbol.replace](string, replaceValue) {
    const rx = requireObj(this, "[Symbol.replace]")
    const S = String(string)
    const lengthS = S.length
    const functionalReplace = typeof replaceValue === "function"
    if (!functionalReplace) replaceValue = String(replaceValue)
    const flags = String(rx.flags)
    const global = flags.includes("g")
    let fullUnicode = false
    if (global) {
      fullUnicode = flags.includes("u") || flags.includes("v")
      setLastIndex(rx, 0)
    }
    const results = []
    for (;;) {
      const result = regExpExec(rx, S)
      if (result === null) break
      results.push(result)
      if (!global) break
      const matchStr = String(result[0])
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
      const matched = String(result[0])
      const matchLength = matched.length
      const position = Math.max(Math.min(reInt(result.index), lengthS), 0)
      const captures = []
      for (let n = 1; n <= nCaptures; n++) {
        let capN = result[n]
        if (capN !== undefined) capN = String(capN)
        captures.push(capN)
      }
      let namedCaptures = result.groups
      let replacement
      if (functionalReplace) {
        const args = [matched, ...captures, position, S]
        if (namedCaptures !== undefined) args.push(namedCaptures)
        replacement = String(replaceValue(...args))
      } else {
        if (namedCaptures !== undefined) namedCaptures = Object(namedCaptures)
        replacement = getSubstitution(matched, S, position, captures, namedCaptures, replaceValue)
      }
      if (position >= nextSourcePosition) {
        accumulated += S.slice(nextSourcePosition, position) + replacement
        nextSourcePosition = position + matchLength
      }
    }
    if (nextSourcePosition >= lengthS) return accumulated
    return accumulated + S.slice(nextSourcePosition)
  },
  [Symbol.search](string) {
    const rx = requireObj(this, "[Symbol.search]")
    const S = String(string)
    const previousLastIndex = rx.lastIndex
    if (!Object.is(previousLastIndex, 0)) setLastIndex(rx, 0)
    const result = regExpExec(rx, S)
    const currentLastIndex = rx.lastIndex
    if (!Object.is(currentLastIndex, previousLastIndex)) setLastIndex(rx, previousLastIndex)
    return result === null ? -1 : result.index
  },
  [Symbol.split](string, limit) {
    const rx = requireObj(this, "[Symbol.split]")
    const S = String(string)
    const C = speciesOf(rx, RegExp)
    const flags = String(rx.flags)
    const unicodeMatching = flags.includes("u") || flags.includes("v")
    const newFlags = flags.includes("y") ? flags : flags + "y"
    const splitter = new C(rx, newFlags)
    const A = []
    let lengthA = 0
    const lim = limit === undefined ? 2 ** 32 - 1 : Number(limit) >>> 0
    if (lim === 0) return A
    const size = S.length
    if (size === 0) {
      const z = regExpExec(splitter, S)
      if (z !== null) return A
      A[0] = S
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
      A[lengthA++] = S.slice(p, q)
      if (lengthA === lim) return A
      p = e
      const numberOfCaptures = Math.max(reLength(z.length) - 1, 0)
      for (let i = 1; i <= numberOfCaptures; i++) {
        A[lengthA++] = z[i]
        if (lengthA === lim) return A
      }
      q = p
    }
    A[lengthA] = S.slice(p, size)
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
"##;

pub const ARRAY_GENERIC: &str = r##"
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
    return typeof f === "function" ? f.call(O) : Object.prototype.toString.call(O)
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
      const le = hasIdx(O, lower)
      const ue = hasIdx(O, upper)
      const lv = le ? O[lower] : undefined
      const uv = ue ? O[upper] : undefined
      if (le && ue) {
        put(O, lower, uv)
        put(O, upper, lv)
      } else if (ue) {
        put(O, lower, uv)
        del(O, upper)
      } else if (le) {
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
