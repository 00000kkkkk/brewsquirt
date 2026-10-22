use super::binary::{buffer_source_bytes, new_uint8};
use super::{arg, method, native_fn};
use crate::compiler::Mode;
use crate::io::IoEvent;
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use std::io::Read;
use std::path::Path;

type R = Result<Value, Value>;

pub const LAZY_GLOBALS: [&str; 1] = ["Buffer"];

const NODE_SRC: &str = r##"
(native) => {
  const encoder = new TextEncoder()
  const decoder = new TextDecoder()

  const ENCODINGS = ["utf8", "utf-8", "hex", "base64", "base64url", "latin1", "binary", "ascii", "utf16le", "utf-16le", "ucs2", "ucs-2"]

  function normEncoding(enc) {
    if (enc == null) return "utf8"
    const e = String(enc).toLowerCase()
    if (e === "utf-8") return "utf8"
    if (e === "binary") return "latin1"
    if (e === "utf-16le" || e === "ucs2" || e === "ucs-2") return "utf16le"
    if (!ENCODINGS.includes(e)) throw new TypeError(`Unknown encoding: ${enc}`)
    return e
  }

  function bytesFromString(str, enc) {
    const e = normEncoding(enc)
    const s = String(str)
    switch (e) {
      case "utf8":
        return encoder.encode(s)
      case "hex": {
        const clean = s.length % 2 === 0 ? s : s.slice(0, -1)
        const out = []
        for (let i = 0; i < clean.length; i += 2) {
          const b = parseInt(clean.slice(i, i + 2), 16)
          if (Number.isNaN(b)) break
          out.push(b)
        }
        return new Uint8Array(out)
      }
      case "base64":
      case "base64url": {
        let t = s.replace(/[^A-Za-z0-9+/\-_]/g, "").replace(/-/g, "+").replace(/_/g, "/")
        if (t.length % 4 === 1) t = t.slice(0, -1)
        while (t.length % 4 !== 0) t += "="
        return Uint8Array.fromBase64(t)
      }
      case "latin1":
      case "ascii": {
        const out = new Uint8Array(s.length)
        for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i) & 255
        return out
      }
      case "utf16le": {
        const out = new Uint8Array(s.length * 2)
        for (let i = 0; i < s.length; i++) {
          const c = s.charCodeAt(i)
          out[i * 2] = c & 255
          out[i * 2 + 1] = c >> 8
        }
        return out
      }
    }
  }

  function stringFromBytes(bytes, enc) {
    const e = normEncoding(enc)
    switch (e) {
      case "utf8":
        return decoder.decode(bytes)
      case "hex":
        return new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength).toHex()
      case "base64":
        return new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength).toBase64()
      case "base64url":
        return new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength).toBase64({ alphabet: "base64url", omitPadding: true })
      case "latin1":
      case "ascii": {
        let out = ""
        const mask = e === "ascii" ? 127 : 255
        for (let i = 0; i < bytes.length; i += 4096) {
          out += String.fromCharCode(...Array.from(bytes.subarray(i, i + 4096), (b) => b & mask))
        }
        return out
      }
      case "utf16le": {
        let out = ""
        for (let i = 0; i + 1 < bytes.length; i += 2) out += String.fromCharCode(bytes[i] | (bytes[i + 1] << 8))
        return out
      }
    }
  }

  class Buffer extends Uint8Array {
    static from(value, encodingOrOffset, length) {
      if (typeof value === "string") return Buffer.#wrap(bytesFromString(value, encodingOrOffset))
      if (value instanceof ArrayBuffer) {
        const offset = encodingOrOffset ?? 0
        return new Buffer(value, offset, length ?? value.byteLength - offset)
      }
      if (ArrayBuffer.isView(value)) {
        return Buffer.#wrap(new Uint8Array(value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength)))
      }
      if (value && value.type === "Buffer" && Array.isArray(value.data)) return new Buffer(value.data)
      if (value != null && (Array.isArray(value) || typeof value[Symbol.iterator] === "function" || typeof value.length === "number")) {
        return new Buffer(value)
      }
      throw new TypeError("The first argument must be of type string or an instance of Buffer, ArrayBuffer, or Array or an Array-like Object.")
    }
    static #wrap(u8) {
      return new Buffer(u8.buffer, u8.byteOffset, u8.byteLength)
    }
    static alloc(size, fill, encoding) {
      const b = new Buffer(size)
      if (fill !== undefined && fill !== 0) b.fill(fill, 0, b.length, encoding)
      return b
    }
    static allocUnsafe(size) {
      return new Buffer(size)
    }
    static allocUnsafeSlow(size) {
      return new Buffer(size)
    }
    static byteLength(value, encoding) {
      if (typeof value !== "string") return value.byteLength
      return bytesFromString(value, encoding).length
    }
    static concat(list, totalLength) {
      const total = totalLength ?? list.reduce((n, b) => n + b.length, 0)
      const out = Buffer.alloc(total)
      let at = 0
      for (const b of list) {
        if (at >= total) break
        const part = b.length > total - at ? b.subarray(0, total - at) : b
        out.set(part, at)
        at += part.length
      }
      return out
    }
    static isBuffer(b) {
      return b instanceof Buffer
    }
    static isEncoding(e) {
      return typeof e === "string" && ENCODINGS.includes(e.toLowerCase())
    }
    static compare(a, b) {
      return a.compare(b)
    }
    toString(encoding, start = 0, end = this.length) {
      return stringFromBytes(this.subarray(start, end), encoding)
    }
    toJSON() {
      return { type: "Buffer", data: Array.from(this) }
    }
    equals(other) {
      return this.compare(other) === 0
    }
    compare(other) {
      const n = Math.min(this.length, other.length)
      for (let i = 0; i < n; i++) {
        if (this[i] !== other[i]) return this[i] < other[i] ? -1 : 1
      }
      return this.length === other.length ? 0 : this.length < other.length ? -1 : 1
    }
    slice(start, end) {
      return this.subarray(start, end)
    }
    write(string, offset = 0, length, encoding) {
      if (typeof offset === "string") [offset, encoding] = [0, offset]
      if (typeof length === "string") [length, encoding] = [undefined, length]
      const bytes = bytesFromString(string, encoding)
      const n = Math.min(bytes.length, length ?? Infinity, this.length - offset)
      this.set(bytes.subarray(0, n), offset)
      return n
    }
    fill(value, offset = 0, end = this.length, encoding) {
      if (typeof offset === "string") [offset, end, encoding] = [0, this.length, offset]
      if (typeof value === "string") {
        const pattern = bytesFromString(value, encoding)
        if (pattern.length === 0) return super.fill(0, offset, end)
        for (let i = offset, j = 0; i < end; i++, j = (j + 1) % pattern.length) this[i] = pattern[j]
        return this
      }
      return super.fill(value, offset, end)
    }
    indexOf(value, byteOffset = 0, encoding) {
      if (typeof value === "number") return super.indexOf(value & 255, byteOffset)
      const needle = typeof value === "string" ? bytesFromString(value, encoding) : value
      const start = byteOffset < 0 ? Math.max(0, this.length + byteOffset) : byteOffset
      outer: for (let i = start; i <= this.length - needle.length; i++) {
        for (let j = 0; j < needle.length; j++) if (this[i + j] !== needle[j]) continue outer
        return i
      }
      return -1
    }
    includes(value, byteOffset, encoding) {
      return this.indexOf(value, byteOffset, encoding) !== -1
    }
    #view() {
      return new DataView(this.buffer, this.byteOffset, this.byteLength)
    }
    readUInt8(o = 0) { return this.#view().getUint8(o) }
    readInt8(o = 0) { return this.#view().getInt8(o) }
    readUInt16LE(o = 0) { return this.#view().getUint16(o, true) }
    readUInt16BE(o = 0) { return this.#view().getUint16(o) }
    readInt16LE(o = 0) { return this.#view().getInt16(o, true) }
    readInt16BE(o = 0) { return this.#view().getInt16(o) }
    readUInt32LE(o = 0) { return this.#view().getUint32(o, true) }
    readUInt32BE(o = 0) { return this.#view().getUint32(o) }
    readInt32LE(o = 0) { return this.#view().getInt32(o, true) }
    readInt32BE(o = 0) { return this.#view().getInt32(o) }
    readFloatLE(o = 0) { return this.#view().getFloat32(o, true) }
    readFloatBE(o = 0) { return this.#view().getFloat32(o) }
    readDoubleLE(o = 0) { return this.#view().getFloat64(o, true) }
    readDoubleBE(o = 0) { return this.#view().getFloat64(o) }
    writeUInt8(v, o = 0) { this.#view().setUint8(o, v); return o + 1 }
    writeInt8(v, o = 0) { this.#view().setInt8(o, v); return o + 1 }
    writeUInt16LE(v, o = 0) { this.#view().setUint16(o, v, true); return o + 2 }
    writeUInt16BE(v, o = 0) { this.#view().setUint16(o, v); return o + 2 }
    writeInt16LE(v, o = 0) { this.#view().setInt16(o, v, true); return o + 2 }
    writeInt16BE(v, o = 0) { this.#view().setInt16(o, v); return o + 2 }
    writeUInt32LE(v, o = 0) { this.#view().setUint32(o, v, true); return o + 4 }
    writeUInt32BE(v, o = 0) { this.#view().setUint32(o, v); return o + 4 }
    writeInt32LE(v, o = 0) { this.#view().setInt32(o, v, true); return o + 4 }
    writeInt32BE(v, o = 0) { this.#view().setInt32(o, v); return o + 4 }
    writeFloatLE(v, o = 0) { this.#view().setFloat32(o, v, true); return o + 4 }
    writeFloatBE(v, o = 0) { this.#view().setFloat32(o, v); return o + 4 }
    writeDoubleLE(v, o = 0) { this.#view().setFloat64(o, v, true); return o + 8 }
    writeDoubleBE(v, o = 0) { this.#view().setFloat64(o, v); return o + 8 }
  }
  for (const name of Object.getOwnPropertyNames(Buffer.prototype)) {
    if (/^(read|write)UInt/.test(name)) {
      Object.defineProperty(Buffer.prototype, name.replace("UInt", "Uint"), { value: Buffer.prototype[name], writable: true, configurable: true })
    }
  }

  const kCapture = Symbol("listeners")
  const listenersOf = (self) => {
    if (!self[kCapture]) Object.defineProperty(self, kCapture, { value: new Map(), writable: true, configurable: true })
    return self[kCapture]
  }

  class EventEmitter {
    static defaultMaxListeners = 10
    constructor() {
      Object.defineProperty(this, kCapture, { value: new Map(), writable: true, configurable: true })
    }
    #add(name, fn, once, prepend) {
      if (typeof fn !== "function") throw new TypeError('The "listener" argument must be of type function')
      if (listenersOf(this).has("newListener") && name !== "newListener") this.emit("newListener", name, fn)
      let list = listenersOf(this).get(name)
      if (!list) listenersOf(this).set(name, (list = []))
      const entry = { fn, once }
      if (prepend) list.unshift(entry)
      else list.push(entry)
      return this
    }
    on(name, fn) {
      return this.#add(name, fn, false, false)
    }
    addListener(name, fn) {
      return this.#add(name, fn, false, false)
    }
    prependListener(name, fn) {
      return this.#add(name, fn, false, true)
    }
    once(name, fn) {
      return this.#add(name, fn, true, false)
    }
    prependOnceListener(name, fn) {
      return this.#add(name, fn, true, true)
    }
    off(name, fn) {
      return this.removeListener(name, fn)
    }
    removeListener(name, fn) {
      const list = listenersOf(this).get(name)
      if (!list) return this
      const i = list.findLastIndex((l) => l.fn === fn)
      if (i >= 0) {
        list.splice(i, 1)
        if (list.length === 0) listenersOf(this).delete(name)
        if (listenersOf(this).has("removeListener")) this.emit("removeListener", name, fn)
      }
      return this
    }
    removeAllListeners(name) {
      if (name === undefined) listenersOf(this).clear()
      else listenersOf(this).delete(name)
      return this
    }
    emit(name, ...args) {
      const list = listenersOf(this).get(name)
      if (!list || list.length === 0) {
        if (name === "error") {
          const err = args[0]
          if (err instanceof Error) throw err
          const e = new Error(`Unhandled error. (${typeof err === "string" ? `'${err}'` : String(err)})`)
          e.context = err
          throw e
        }
        return false
      }
      for (const l of [...list]) {
        if (l.once) this.removeListener(name, l.fn)
        l.fn.apply(this, args)
      }
      return true
    }
    listenerCount(name) {
      return listenersOf(this).get(name)?.length ?? 0
    }
    listeners(name) {
      return (listenersOf(this).get(name) ?? []).map((l) => l.fn)
    }
    rawListeners(name) {
      return this.listeners(name)
    }
    eventNames() {
      return [...listenersOf(this).keys()]
    }
    setMaxListeners() {
      return this
    }
    getMaxListeners() {
      return EventEmitter.defaultMaxListeners
    }
    static once(emitter, name) {
      return new Promise((resolve, reject) => {
        const onError = (err) => {
          emitter.removeListener(name, onEvent)
          reject(err)
        }
        const onEvent = (...args) => {
          if (name !== "error") emitter.removeListener("error", onError)
          resolve(args)
        }
        emitter.once(name, onEvent)
        if (name !== "error") emitter.once("error", onError)
      })
    }
    static async *on(emitter, name) {
      const queue = []
      let wake = null
      emitter.on(name, (...args) => {
        queue.push(args)
        if (wake) wake()
      })
      while (true) {
        if (queue.length === 0) await new Promise((r) => (wake = r))
        wake = null
        yield queue.shift()
      }
    }
    static listenerCount(emitter, name) {
      return emitter.listenerCount(name)
    }
  }

  function encodingOf(options) {
    if (typeof options === "string") return options
    if (options && typeof options === "object") return options.encoding ?? null
    return null
  }

  function toData(data, options) {
    if (typeof data === "string") {
      const enc = encodingOf(options)
      return enc && normEncoding(enc) !== "utf8" ? bytesFromString(data, enc) : data
    }
    if (ArrayBuffer.isView(data) || data instanceof ArrayBuffer) return data
    return String(data)
  }

  class Stats {
    constructor(raw) {
      this.dev = 0
      this.mode = raw.mode
      this.nlink = 1
      this.uid = 0
      this.gid = 0
      this.rdev = 0
      this.blksize = 4096
      this.ino = 0
      this.size = raw.size
      this.blocks = Math.ceil(raw.size / 512)
      this.atimeMs = raw.atimeMs
      this.mtimeMs = raw.mtimeMs
      this.ctimeMs = raw.ctimeMs
      this.birthtimeMs = raw.birthtimeMs
      Object.defineProperty(this, "kind", { value: raw.kind })
    }
    get atime() { return new Date(this.atimeMs) }
    get mtime() { return new Date(this.mtimeMs) }
    get ctime() { return new Date(this.ctimeMs) }
    get birthtime() { return new Date(this.birthtimeMs) }
    isFile() { return this.kind === "file" }
    isDirectory() { return this.kind === "dir" }
    isSymbolicLink() { return this.kind === "symlink" }
    isFIFO() { return false }
    isSocket() { return false }
    isBlockDevice() { return false }
    isCharacterDevice() { return false }
  }

  class Dirent {
    constructor(name, kind, parentPath) {
      this.name = name
      this.parentPath = parentPath
      this.path = parentPath
      Object.defineProperty(this, "kind", { value: kind })
    }
    isFile() { return this.kind === "file" }
    isDirectory() { return this.kind === "dir" }
    isSymbolicLink() { return this.kind === "symlink" }
    isFIFO() { return false }
    isSocket() { return false }
    isBlockDevice() { return false }
    isCharacterDevice() { return false }
  }

  const pathOf = (p) => (p instanceof URL ? fileURLToPath(p) : typeof p === "number" ? p : String(p))

  const fs = {
    readFileSync(path, options) {
      const bytes = native.read(pathOf(path))
      const enc = encodingOf(options)
      return enc ? stringFromBytes(bytes, enc) : Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength)
    },
    writeFileSync(path, data, options) {
      native.write(pathOf(path), toData(data, options), options?.flag === "a")
    },
    appendFileSync(path, data, options) {
      native.write(pathOf(path), toData(data, options), true)
    },
    existsSync(path) {
      try {
        return native.exists(pathOf(path))
      } catch {
        return false
      }
    },
    accessSync(path) {
      native.access(pathOf(path))
    },
    statSync(path, options) {
      try {
        return new Stats(native.stat(pathOf(path), true))
      } catch (e) {
        if (options?.throwIfNoEntry === false && e.code === "ENOENT") return undefined
        throw e
      }
    },
    lstatSync(path, options) {
      try {
        return new Stats(native.stat(pathOf(path), false))
      } catch (e) {
        if (options?.throwIfNoEntry === false && e.code === "ENOENT") return undefined
        throw e
      }
    },
    readdirSync(path, options) {
      const p = pathOf(path)
      const entries = native.readdir(p, !!options?.recursive)
      if (options?.withFileTypes) {
        return entries.map(([name, kind, parent]) => new Dirent(name, kind, parent))
      }
      return entries.map(([name, , , rel]) => rel)
    },
    mkdirSync(path, options) {
      const recursive = typeof options === "object" ? !!options?.recursive : false
      return native.mkdir(pathOf(path), recursive)
    },
    mkdtempSync(prefix) {
      return native.mkdtemp(String(prefix))
    },
    rmSync(path, options) {
      native.rm(pathOf(path), !!options?.recursive, !!options?.force)
    },
    rmdirSync(path, options) {
      native.rmdir(pathOf(path), !!options?.recursive)
    },
    unlinkSync(path) {
      native.unlink(pathOf(path))
    },
    renameSync(from, to) {
      native.rename(pathOf(from), pathOf(to))
    },
    copyFileSync(from, to) {
      native.copy(pathOf(from), pathOf(to))
    },
    cpSync(from, to, options) {
      native.cp(pathOf(from), pathOf(to), !!options?.recursive)
    },
    realpathSync(path) {
      return native.realpath(pathOf(path))
    },
    symlinkSync(target, path) {
      native.symlink(pathOf(target), pathOf(path))
    },
    readlinkSync(path) {
      return native.readlink(pathOf(path))
    },
    chmodSync() {},
    utimesSync() {},
    constants: { F_OK: 0, R_OK: 4, W_OK: 2, X_OK: 1 },
    Stats,
    Dirent,
  }

  const promises = {}
  for (const [name, fn] of Object.entries(fs)) {
    if (typeof fn === "function" && name.endsWith("Sync")) {
      const short = name.slice(0, -4)
      promises[short] = async (...args) => fn(...args)
      fs[short] = (...args) => {
        const cb = typeof args.at(-1) === "function" ? args.pop() : null
        let result
        let error = null
        try {
          result = fn(...args)
        } catch (e) {
          error = e
        }
        if (cb) queueMicrotask(() => (short === "exists" ? cb(!error && result) : cb(error, result)))
      }
    }
  }
  promises.constants = fs.constants
  fs.promises = promises

  function fileURLToPath(url) {
    const u = typeof url === "string" ? new URL(url) : url
    if (u.protocol !== "file:") throw new TypeError("The URL must be of scheme file")
    let p = decodeURIComponent(u.pathname)
    if (process.platform === "win32") {
      if (/^\/[A-Za-z]:/.test(p)) p = p.slice(1)
      p = p.replace(/\//g, "\\")
      if (u.hostname) p = `\\\\${u.hostname}${p}`
    }
    return p
  }

  function pathToFileURL(path) {
    let p = native.resolve(String(path))
    if (process.platform === "win32") p = "/" + p.replace(/\\/g, "/")
    const url = new URL("file://")
    url.pathname = p
    return url
  }

  class AssertionError extends Error {
    constructor(options) {
      super(options.message)
      this.name = "AssertionError"
      this.code = "ERR_ASSERTION"
      this.actual = options.actual
      this.expected = options.expected
      this.operator = options.operator
      this.generatedMessage = !options.userMessage
    }
  }

  function deepEqual(a, b, strict) {
    if (strict ? Object.is(a, b) : a == b || Object.is(a, b)) return true
    if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false
    if (strict && Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false
    if (a instanceof Date && b instanceof Date) return a.getTime() === b.getTime()
    if (a instanceof RegExp && b instanceof RegExp) return String(a) === String(b)
    if (a instanceof Map && b instanceof Map) {
      if (a.size !== b.size) return false
      for (const [k, v] of a) if (!b.has(k) || !deepEqual(v, b.get(k), strict)) return false
      return true
    }
    if (a instanceof Set && b instanceof Set) {
      if (a.size !== b.size) return false
      for (const v of a) if (!b.has(v)) return false
      return true
    }
    if (ArrayBuffer.isView(a) && ArrayBuffer.isView(b)) {
      if (a.length !== b.length) return false
      for (let i = 0; i < a.length; i++) if (!Object.is(a[i], b[i])) return false
      return true
    }
    if (Array.isArray(a) !== Array.isArray(b)) return false
    const ka = Object.keys(a)
    const kb = Object.keys(b)
    if (ka.length !== kb.length) return false
    for (const k of ka) if (!Object.hasOwn(b, k) || !deepEqual(a[k], b[k], strict)) return false
    return true
  }

  const inspect = (v) => native.inspect(v)

  function fail(actual, expected, message, operator, defaultMessage) {
    if (message instanceof Error) throw message
    throw new AssertionError({ message: message ?? defaultMessage, actual, expected, operator, userMessage: message !== undefined })
  }

  function matchesError(error, expected) {
    if (expected === undefined) return true
    if (typeof expected === "function") {
      if (expected.prototype !== undefined && error instanceof expected) return true
      if (Error.isPrototypeOf(expected) || expected === Error) return false
      return expected(error) === true
    }
    if (expected instanceof RegExp) return expected.test(String(error?.message ?? error))
    if (typeof expected === "object") {
      return Object.keys(expected).every((k) =>
        expected[k] instanceof RegExp && typeof error[k] === "string" ? expected[k].test(error[k]) : deepEqual(error[k], expected[k], true),
      )
    }
    return false
  }

  function assert(value, message) {
    if (!value) fail(value, true, message, "==", "The expression evaluated to a falsy value:\n\n  assert(" + inspect(value) + ")\n")
  }
  assert.ok = (value, message) => {
    if (!value) fail(value, true, message, "==", "The expression evaluated to a falsy value:\n\n  assert.ok(" + inspect(value) + ")\n")
  }
  assert.equal = (a, b, message) => {
    if (!(a == b || (a !== a && b !== b))) fail(a, b, message, "==", `${inspect(a)} == ${inspect(b)}`)
  }
  assert.notEqual = (a, b, message) => {
    if (a == b) fail(a, b, message, "!=", `${inspect(a)} != ${inspect(b)}`)
  }
  assert.strictEqual = (a, b, message) => {
    if (!Object.is(a, b)) fail(a, b, message, "strictEqual", `Expected values to be strictly equal:\n\n${inspect(a)} !== ${inspect(b)}\n`)
  }
  assert.notStrictEqual = (a, b, message) => {
    if (Object.is(a, b)) fail(a, b, message, "notStrictEqual", `Expected "actual" to be strictly unequal to: ${inspect(b)}`)
  }
  assert.deepEqual = (a, b, message) => {
    if (!deepEqual(a, b, false)) fail(a, b, message, "deepEqual", `Expected values to be loosely deep-equal:\n\n${inspect(a)}\n\nshould loosely deep-equal\n\n${inspect(b)}`)
  }
  assert.deepStrictEqual = (a, b, message) => {
    if (!deepEqual(a, b, true)) fail(a, b, message, "deepStrictEqual", `Expected values to be strictly deep-equal:\n${inspect(a)}\n\nshould equal\n\n${inspect(b)}`)
  }
  assert.notDeepStrictEqual = (a, b, message) => {
    if (deepEqual(a, b, true)) fail(a, b, message, "notDeepStrictEqual", `Expected "actual" not to be strictly deep-equal to: ${inspect(b)}`)
  }
  assert.notDeepEqual = (a, b, message) => {
    if (deepEqual(a, b, false)) fail(a, b, message, "notDeepEqual", `Expected "actual" not to be loosely deep-equal to: ${inspect(b)}`)
  }
  assert.throws = (fn, expected, message) => {
    if (typeof expected === "string") [expected, message] = [undefined, expected]
    try {
      fn()
    } catch (e) {
      if (!matchesError(e, expected)) throw e
      return
    }
    fail(undefined, expected, message, "throws", "Missing expected exception.")
  }
  assert.doesNotThrow = (fn, message) => {
    try {
      fn()
    } catch (e) {
      fail(e, undefined, typeof message === "string" ? message : undefined, "doesNotThrow", `Got unwanted exception.\nActual message: "${e?.message}"`)
    }
  }
  assert.rejects = async (promiseOrFn, expected, message) => {
    if (typeof expected === "string") [expected, message] = [undefined, expected]
    try {
      await (typeof promiseOrFn === "function" ? promiseOrFn() : promiseOrFn)
    } catch (e) {
      if (!matchesError(e, expected)) throw e
      return
    }
    fail(undefined, expected, message, "rejects", "Missing expected rejection.")
  }
  assert.doesNotReject = async (promiseOrFn, message) => {
    try {
      await (typeof promiseOrFn === "function" ? promiseOrFn() : promiseOrFn)
    } catch (e) {
      fail(e, undefined, message, "doesNotReject", `Got unwanted rejection.\nActual message: "${e?.message}"`)
    }
  }
  assert.match = (s, re, message) => {
    if (!re.test(s)) fail(s, re, message, "match", `The input did not match the regular expression ${re}. Input:\n\n${inspect(s)}\n`)
  }
  assert.doesNotMatch = (s, re, message) => {
    if (re.test(s)) fail(s, re, message, "doesNotMatch", `The input was expected to not match the regular expression ${re}. Input:\n\n${inspect(s)}\n`)
  }
  assert.fail = (message = "Failed") => {
    if (message instanceof Error) throw message
    throw new AssertionError({ message, operator: "fail", userMessage: true })
  }
  assert.ifError = (value) => {
    if (value !== null && value !== undefined) throw value
  }
  assert.AssertionError = AssertionError
  assert.strict = assert

  function promisify(fn) {
    if (typeof fn !== "function") throw new TypeError('The "original" argument must be of type function')
    return function (...args) {
      return new Promise((resolve, reject) => {
        fn.call(this, ...args, (err, ...values) => {
          if (err) reject(err)
          else resolve(values.length > 1 ? values : values[0])
        })
      })
    }
  }

  function callbackify(fn) {
    return function (...args) {
      const cb = args.pop()
      fn.apply(this, args).then((v) => cb(null, v), (e) => cb(e))
    }
  }

  function deprecate(fn) {
    return fn
  }

  function inherits(ctor, superCtor) {
    Object.setPrototypeOf(ctor.prototype, superCtor.prototype)
    Object.setPrototypeOf(ctor, superCtor)
  }

  const types = {
    isDate: (v) => v instanceof Date,
    isRegExp: (v) => v instanceof RegExp,
    isMap: (v) => v instanceof Map,
    isSet: (v) => v instanceof Set,
    isPromise: (v) => v instanceof Promise,
    isTypedArray: (v) => ArrayBuffer.isView(v) && !(v instanceof DataView),
    isUint8Array: (v) => v instanceof Uint8Array,
    isArrayBuffer: (v) => v instanceof ArrayBuffer,
    isNativeError: (v) => v instanceof Error,
    isAsyncFunction: (v) => typeof v === "function" && Object.prototype.toString.call(v) === "[object AsyncFunction]",
  }

  const stdin = new EventEmitter()
  let stdinEncoding = null
  let stdinStarted = false
  let stdinEnded = false
  const stdinChunks = []
  let stdinWake = null
  stdin.fd = 0
  stdin.isTTY = native.isTTY(0)
  stdin.setEncoding = (enc) => {
    stdinEncoding = enc
    return stdin
  }
  const startStdin = () => {
    if (stdinStarted) return
    stdinStarted = true
    native.stdin((chunk) => {
      if (chunk === null) {
        stdinEnded = true
        if (stdinWake) stdinWake()
        stdin.emit("end")
        stdin.emit("close")
        return
      }
      const buf = Buffer.from(chunk.buffer, chunk.byteOffset, chunk.byteLength)
      stdinChunks.push(buf)
      if (stdinWake) stdinWake()
      if (stdin.listenerCount("data") > 0) stdin.emit("data", stdinEncoding ? buf.toString(stdinEncoding) : buf)
    })
  }
  const baseOn = stdin.on.bind(stdin)
  stdin.on = stdin.addListener = (name, fn) => {
    baseOn(name, fn)
    if (name === "data" || name === "readable" || name === "end") startStdin()
    return stdin
  }
  stdin.resume = () => {
    startStdin()
    return stdin
  }
  stdin.pause = () => {
    native.stdinStop()
    return stdin
  }
  stdin.unref = () => {
    native.stdinStop()
    return stdin
  }
  stdin.ref = () => stdin
  stdin.read = () => {
    if (stdinChunks.length === 0) return null
    const all = Buffer.concat(stdinChunks.splice(0))
    return stdinEncoding ? all.toString(stdinEncoding) : all
  }
  stdin[Symbol.asyncIterator] = async function* () {
    startStdin()
    while (true) {
      if (stdinChunks.length > 0) {
        const c = stdinChunks.shift()
        yield stdinEncoding ? c.toString(stdinEncoding) : c
        continue
      }
      if (stdinEnded) return
      await new Promise((r) => (stdinWake = r))
      stdinWake = null
    }
  }

  class ReadStream extends EventEmitter {
    #encoding = null
    #destroyed = false
    #paused = false
    #started = false
    #flowing = false
    #chunks = []
    constructor(path, options = {}) {
      super()
      if (typeof options === "string") options = { encoding: options }
      this.path = path
      this.bytesRead = 0
      this.readable = true
      this.highWaterMark = options.highWaterMark ?? 65536
      if (options.encoding) this.#encoding = options.encoding
      queueMicrotask(() => this.#open(options))
    }
    #open(options) {
      if (this.#destroyed) return
      let bytes
      try {
        bytes = native.read(pathOf(this.path))
      } catch (e) {
        this.destroy(e)
        return
      }
      const start = options.start ?? 0
      const end = options.end === undefined ? bytes.length : Math.min(bytes.length, options.end + 1)
      bytes = bytes.subarray(start, end)
      if (this.#encoding) {
        if (bytes.length) this.#chunks.push(stringFromBytes(bytes, this.#encoding))
      } else {
        for (let i = 0; i < bytes.length; i += this.highWaterMark) {
          const c = bytes.subarray(i, i + this.highWaterMark)
          this.#chunks.push(Buffer.from(c.buffer, c.byteOffset, c.byteLength))
        }
      }
      this.#started = true
      this.emit("open", 0)
      this.emit("ready")
      this.#flow()
    }
    #flow() {
      if (!this.#started || this.#paused || this.#destroyed || this.#flowing) return
      if (this.listenerCount("data") === 0) return
      this.#flowing = true
      queueMicrotask(() => {
        this.#flowing = false
        while (this.#chunks.length && !this.#paused && !this.#destroyed) {
          const c = this.#chunks.shift()
          this.bytesRead += typeof c === "string" ? Buffer.byteLength(c, this.#encoding) : c.length
          this.emit("data", c)
        }
        if (!this.#chunks.length && !this.#destroyed) {
          this.readable = false
          this.emit("end")
          this.destroy()
        }
      })
    }
    on(event, fn) {
      super.on(event, fn)
      if (event === "data") this.#flow()
      return this
    }
    setEncoding(encoding) {
      this.#encoding = encoding ?? "utf8"
      this.#chunks = this.#chunks.map((c) => (typeof c === "string" ? c : c.toString(this.#encoding)))
      return this
    }
    pause() {
      this.#paused = true
      return this
    }
    resume() {
      this.#paused = false
      this.#flow()
      return this
    }
    isPaused() {
      return this.#paused
    }
    destroy(err) {
      if (this.#destroyed) return this
      this.#destroyed = true
      this.readable = false
      this.#chunks = []
      queueMicrotask(() => {
        if (err) this.emit("error", err)
        this.emit("close")
      })
      return this
    }
    close(cb) {
      if (typeof cb === "function") this.once("close", cb)
      this.destroy()
    }
    pipe(dest, options) {
      this.on("data", (chunk) => {
        if (dest.write(chunk) === false && typeof dest.once === "function") {
          this.pause()
          dest.once("drain", () => this.resume())
        }
      })
      if (options?.end !== false) this.on("end", () => dest.end())
      return dest
    }
    async *[Symbol.asyncIterator]() {
      const queue = []
      let done = false
      let error = null
      let wake = null
      this.on("data", (c) => {
        queue.push(c)
        if (wake) wake()
      })
      this.on("end", () => {
        done = true
        if (wake) wake()
      })
      this.on("error", (e) => {
        error = e
        if (wake) wake()
      })
      while (true) {
        if (queue.length) {
          yield queue.shift()
          continue
        }
        if (error) throw error
        if (done) return
        await new Promise((r) => (wake = r))
        wake = null
      }
    }
  }

  class WriteStream extends EventEmitter {
    #ended = false
    #destroyed = false
    constructor(path, options = {}) {
      super()
      if (typeof options === "string") options = { encoding: options }
      this.path = path
      this.bytesWritten = 0
      this.writable = true
      this.pending = false
      this.encoding = options.encoding ?? "utf8"
      const append = String(options.flags ?? "w").startsWith("a")
      try {
        if (!append) native.write(pathOf(path), new Uint8Array(0), false)
        else if (!fs.existsSync(path)) native.write(pathOf(path), new Uint8Array(0), false)
      } catch (e) {
        queueMicrotask(() => this.destroy(e))
        return
      }
      queueMicrotask(() => {
        this.emit("open", 1)
        this.emit("ready")
      })
    }
    write(chunk, encoding, cb) {
      if (typeof encoding === "function") [encoding, cb] = [undefined, encoding]
      if (this.#ended || this.#destroyed) {
        const e = new Error("write after end")
        e.code = "ERR_STREAM_WRITE_AFTER_END"
        queueMicrotask(() => {
          if (typeof cb === "function") cb(e)
          this.emit("error", e)
        })
        return false
      }
      const data = toData(chunk, { encoding: encoding ?? this.encoding })
      try {
        native.write(pathOf(this.path), data, true)
      } catch (e) {
        queueMicrotask(() => {
          if (typeof cb === "function") cb(e)
          this.destroy(e)
        })
        return false
      }
      this.bytesWritten += typeof data === "string" ? Buffer.byteLength(data) : data.byteLength
      if (typeof cb === "function") queueMicrotask(() => cb(null))
      return true
    }
    end(chunk, encoding, cb) {
      if (typeof chunk === "function") [chunk, encoding, cb] = [undefined, undefined, chunk]
      if (typeof encoding === "function") [encoding, cb] = [undefined, encoding]
      if (chunk != null) this.write(chunk, encoding)
      if (typeof cb === "function") this.once("finish", cb)
      if (!this.#ended) {
        this.#ended = true
        this.writable = false
        queueMicrotask(() => {
          this.emit("finish")
          this.destroy()
        })
      }
      return this
    }
    destroy(err) {
      if (this.#destroyed) return this
      this.#destroyed = true
      this.writable = false
      queueMicrotask(() => {
        if (err) this.emit("error", err)
        this.emit("close")
      })
      return this
    }
    close(cb) {
      return this.end(cb)
    }
    cork() {}
    uncork() {}
    setDefaultEncoding(encoding) {
      this.encoding = encoding
      return this
    }
  }

  fs.createReadStream = (path, options) => new ReadStream(path, options)
  fs.createWriteStream = (path, options) => new WriteStream(path, options)
  fs.ReadStream = ReadStream
  fs.WriteStream = WriteStream

  class Interface extends EventEmitter {
    #input
    #closed = false
    #buffer = ""
    #lines = []
    #wake = null
    #prompt = "> "
    #pendingQuestion = null
    constructor(input, output) {
      super()
      this.#input = input
      this.output = output
      this.terminal = false
      this.line = ""
      input.on("data", (chunk) => this.#feed(typeof chunk === "string" ? chunk : chunk.toString("utf8")))
      input.on("end", () => {
        if (this.#buffer.length > 0) {
          this.#emitLine(this.#buffer)
          this.#buffer = ""
        }
        this.close()
      })
    }
    #feed(text) {
      this.#buffer += text
      let i
      while ((i = this.#buffer.indexOf("\n")) >= 0) {
        let line = this.#buffer.slice(0, i)
        if (line.endsWith("\r")) line = line.slice(0, -1)
        this.#buffer = this.#buffer.slice(i + 1)
        this.#emitLine(line)
      }
    }
    #emitLine(line) {
      if (this.#pendingQuestion) {
        const q = this.#pendingQuestion
        this.#pendingQuestion = null
        q(line)
        return
      }
      if (this.listenerCount("line") === 0) this.#lines.push(line)
      else this.emit("line", line)
      if (this.#wake) this.#wake()
    }
    setPrompt(p) {
      this.#prompt = String(p)
    }
    getPrompt() {
      return this.#prompt
    }
    prompt() {
      if (this.output) this.output.write(this.#prompt)
    }
    question(query, options, cb) {
      if (typeof options === "function") cb = options
      if (this.output) this.output.write(String(query))
      if (this.#lines.length > 0) {
        const line = this.#lines.shift()
        queueMicrotask(() => cb(line))
      } else {
        this.#pendingQuestion = cb
      }
    }
    write(data) {
      if (this.output) this.output.write(String(data))
    }
    close() {
      if (this.#closed) return
      this.#closed = true
      if (this.#input === stdin) stdin.pause()
      if (this.#wake) this.#wake()
      this.emit("close")
    }
    pause() {
      return this
    }
    resume() {
      return this
    }
    async *[Symbol.asyncIterator]() {
      const own = this.#lines.splice(0)
      const take = (line) => own.push(line)
      this.on("line", take)
      try {
        while (true) {
          if (own.length > 0) {
            yield own.shift()
            continue
          }
          if (this.#closed) return
          await new Promise((r) => (this.#wake = r))
          this.#wake = null
        }
      } finally {
        this.off("line", take)
      }
    }
  }

  function createInterface(input, output) {
    if (input && typeof input === "object" && !("on" in input && typeof input.on === "function" && input !== stdin && !(input instanceof EventEmitter))) {
      if ("input" in input) return new Interface(input.input, input.output)
    }
    if (input && input.input) return new Interface(input.input, input.output)
    return new Interface(input, output)
  }

  const readlinePromises = {
    createInterface(options) {
      const rl = createInterface(options)
      const q = rl.question.bind(rl)
      rl.question = (query) => new Promise((resolve) => q(query, resolve))
      return rl
    },
  }

  const hashSizes = { md5: 16, sha1: 20, sha224: 28, sha256: 32, sha384: 48, sha512: 64 }
  function hashName(alg) {
    const key = String(alg).toLowerCase().replace(/-/g, "")
    if (!(key in hashSizes)) throw new Error(`Digest method not supported: ${alg}`)
    return key
  }
  function cryptoBytes(data, encoding, what = "data") {
    if (typeof data === "string") return Buffer.from(data, encoding)
    if (data instanceof ArrayBuffer) return new Uint8Array(data)
    if (ArrayBuffer.isView(data)) return new Uint8Array(data.buffer, data.byteOffset, data.byteLength)
    throw new TypeError(`The "${what}" argument must be of type string or an instance of Buffer, TypedArray, or DataView`)
  }
  function concatBytes(parts) {
    if (parts.length === 1) return parts[0]
    const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0))
    let at = 0
    for (const p of parts) {
      out.set(p, at)
      at += p.length
    }
    return out
  }
  function encodeDigest(bytes, encoding) {
    const buf = Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength)
    return encoding === undefined || encoding === "buffer" ? buf : buf.toString(encoding)
  }
  class Hash {
    #alg
    #parts = []
    #done = false
    constructor(alg) {
      this.#alg = hashName(alg)
    }
    update(data, encoding) {
      if (this.#done) throw new Error("Digest already called")
      this.#parts.push(cryptoBytes(data, encoding))
      return this
    }
    digest(encoding) {
      if (this.#done) throw new Error("Digest already called")
      this.#done = true
      return encodeDigest(native.digest(this.#alg, concatBytes(this.#parts)), encoding)
    }
    copy() {
      if (this.#done) throw new Error("Digest already called")
      const h = new Hash(this.#alg)
      for (const p of this.#parts) h.update(p)
      return h
    }
  }
  class Hmac {
    #alg
    #key
    #parts = []
    #done = false
    constructor(alg, key, encoding) {
      this.#alg = hashName(alg)
      this.#key = cryptoBytes(key, encoding, "key")
    }
    update(data, encoding) {
      if (this.#done) throw new Error("Digest already called")
      this.#parts.push(cryptoBytes(data, encoding))
      return this
    }
    digest(encoding) {
      if (this.#done) throw new Error("Digest already called")
      this.#done = true
      return encodeDigest(native.hmac(this.#alg, this.#key, concatBytes(this.#parts)), encoding)
    }
  }
  function randomInt(min, max, cb) {
    if (typeof max === "function" || max === undefined) [min, max, cb] = [0, min, max]
    if (!Number.isSafeInteger(min)) throw new TypeError('The "min" argument must be a safe integer')
    if (!Number.isSafeInteger(max)) throw new TypeError('The "max" argument must be a safe integer')
    if (max <= min) throw new RangeError(`The value of "max" is out of range. It must be greater than the value of "min" (${min}). Received ${max}`)
    const range = max - min
    const limit = 2 ** 48 - (2 ** 48 % range)
    let x
    do {
      const bytes = native.randomBytes(6)
      x = 0
      for (const b of bytes) x = x * 256 + b
    } while (x >= limit)
    const value = min + (x % range)
    if (typeof cb === "function") {
      queueMicrotask(() => cb(null, value))
      return
    }
    return value
  }
  const crypto = {
    createHash: (alg) => new Hash(alg),
    createHmac: (alg, key, options) => new Hmac(alg, key, options?.encoding),
    hash: (alg, data, encoding = "hex") => new Hash(alg).update(data).digest(encoding),
    getHashes: () => Object.keys(hashSizes),
    randomBytes(size, cb) {
      const buf = Buffer.from(native.randomBytes(size).buffer)
      if (typeof cb === "function") {
        queueMicrotask(() => cb(null, buf))
        return
      }
      return buf
    },
    randomFillSync(buffer, offset = 0, size) {
      const view = cryptoBytes(buffer, undefined, "buffer")
      const n = size ?? view.length - offset
      view.set(native.randomBytes(n), offset)
      return buffer
    },
    randomInt,
    randomUUID: () => globalThis.crypto.randomUUID(),
    getRandomValues: (array) => globalThis.crypto.getRandomValues(array),
    timingSafeEqual(a, b) {
      const x = cryptoBytes(a, undefined, "buf1")
      const y = cryptoBytes(b, undefined, "buf2")
      if (x.length !== y.length) throw new RangeError("Input buffers must have the same byte length")
      let diff = 0
      for (let i = 0; i < x.length; i++) diff |= x[i] ^ y[i]
      return diff === 0
    },
    Hash,
    Hmac,
    webcrypto: globalThis.crypto,
    subtle: globalThis.crypto.subtle,
    constants: {},
  }
  const utf8Incomplete = (bytes) => {
    const n = bytes.length
    for (let back = 1; back <= 3 && back <= n; back++) {
      const b = bytes[n - back]
      if (b < 0x80) return 0
      if (b >= 0xc0) {
        const need = b >= 0xf0 ? 4 : b >= 0xe0 ? 3 : 2
        return back < need ? back : 0
      }
    }
    return 0
  }
  class StringDecoder {
    #pending = new Uint8Array(0)
    constructor(encoding = "utf8") {
      this.encoding = normEncoding(encoding)
    }
    #join(buf) {
      const bytes = cryptoBytes(buf, undefined, "buf")
      if (this.#pending.length === 0) return bytes
      const out = new Uint8Array(this.#pending.length + bytes.length)
      out.set(this.#pending)
      out.set(bytes, this.#pending.length)
      this.#pending = new Uint8Array(0)
      return out
    }
    write(buf) {
      if (typeof buf === "string") return buf
      const bytes = this.#join(buf)
      let keep = 0
      if (this.encoding === "utf8") keep = utf8Incomplete(bytes)
      else if (this.encoding === "utf16le") keep = bytes.length % 2
      else if (this.encoding === "base64" || this.encoding === "base64url") keep = bytes.length % 3
      if (keep) {
        this.#pending = bytes.slice(bytes.length - keep)
        return stringFromBytes(bytes.subarray(0, bytes.length - keep), this.encoding)
      }
      return stringFromBytes(bytes, this.encoding)
    }
    end(buf) {
      let out = buf === undefined ? "" : this.write(buf)
      if (this.#pending.length) {
        out += stringFromBytes(this.#pending, this.encoding)
        this.#pending = new Uint8Array(0)
      }
      return out
    }
    get lastNeed() {
      return this.#pending.length
    }
  }

  const qsEscape = (s) => encodeURIComponent(s).replace(/[!'()*]/g, (c) => "%" + c.charCodeAt(0).toString(16).toUpperCase())
  const qsUnescape = (s) => {
    try {
      return decodeURIComponent(s.replace(/\+/g, " "))
    } catch {
      return s
    }
  }
  const qsValue = (v) => {
    if (typeof v === "string") return v
    if (typeof v === "number" && Number.isFinite(v)) return String(v)
    if (typeof v === "bigint" || typeof v === "boolean") return String(v)
    return ""
  }
  const querystring = {
    escape: qsEscape,
    unescape: qsUnescape,
    parse(str, sep = "&", eq = "=", options) {
      const out = Object.create(null)
      if (typeof str !== "string" || str.length === 0) return out
      const decode = options?.decodeURIComponent ?? qsUnescape
      let count = 0
      const max = options?.maxKeys ?? 1000
      for (const part of str.split(sep)) {
        if (part === "") continue
        if (max > 0 && count++ >= max) break
        const i = part.indexOf(eq)
        const k = decode(i < 0 ? part : part.slice(0, i))
        const v = i < 0 ? "" : decode(part.slice(i + eq.length))
        if (k in out) {
          if (Array.isArray(out[k])) out[k].push(v)
          else out[k] = [out[k], v]
        } else out[k] = v
      }
      return out
    },
    stringify(obj, sep = "&", eq = "=", options) {
      if (obj === null || typeof obj !== "object") return ""
      const encode = options?.encodeURIComponent ?? qsEscape
      const parts = []
      for (const k of Object.keys(obj)) {
        const v = obj[k]
        const ks = encode(qsValue(k)) + eq
        if (Array.isArray(v)) for (const x of v) parts.push(ks + encode(qsValue(x)))
        else parts.push(ks + encode(qsValue(v)))
      }
      return parts.join(sep)
    },
  }
  querystring.encode = querystring.stringify
  querystring.decode = querystring.parse

  const tty = {
    isatty: (fd) => Number.isInteger(fd) && fd >= 0 && native.isTTY(fd),
  }

  class PerformanceObserver {
    #cb
    constructor(cb) {
      this.#cb = cb
    }
    observe() {}
    disconnect() {}
    takeRecords() {
      return []
    }
    static get supportedEntryTypes() {
      return ["mark", "measure"]
    }
  }

  const kRead = Symbol("readableState")
  const kWrite = Symbol("writableState")
  const streamError = (code, message) => {
    const e = new Error(message)
    e.code = code
    return e
  }
  function destroyStream(stream, err) {
    const r = stream[kRead]
    const w = stream[kWrite]
    if ((r && r.destroyed) || (w && w.destroyed)) return stream
    if (r) {
      r.destroyed = true
      r.buffer = []
      stream.readable = false
    }
    if (w) {
      w.destroyed = true
      w.queue = []
      stream.writable = false
    }
    stream.destroyed = true
    const done = (e) => {
      const error = e === undefined ? err : e
      queueMicrotask(() => {
        if (error) stream.emit("error", error)
        stream.emit("close")
      })
    }
    if (typeof stream._destroy === "function") {
      try {
        stream._destroy(err ?? null, done)
      } catch (e) {
        done(e)
      }
    } else done()
    return stream
  }
  function toChunk(stream, chunk, encoding, objectMode) {
    if (objectMode) return chunk
    if (typeof chunk === "string") return Buffer.from(chunk, encoding ?? "utf8")
    if (chunk instanceof Uint8Array) return Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk.buffer, chunk.byteOffset, chunk.byteLength)
    if (chunk instanceof ArrayBuffer) return Buffer.from(chunk)
    throw streamError("ERR_INVALID_ARG_TYPE", `The "chunk" argument must be of type string or an instance of Buffer or Uint8Array. Received ${typeof chunk}`)
  }

  class Readable extends EventEmitter {
    constructor(options = {}) {
      super()
      this[kRead] = {
        buffer: [],
        ended: false,
        endEmitted: false,
        flowing: null,
        reading: false,
        scheduled: false,
        destroyed: false,
        encoding: options.encoding ?? null,
        objectMode: !!options.objectMode,
        highWaterMark: options.highWaterMark ?? (options.objectMode ? 16 : 65536),
        autoDestroy: options.autoDestroy !== false,
        pipes: [],
      }
      this.readable = true
      this.readableEnded = false
      this.destroyed = false
      if (typeof options.read === "function") this._read = options.read
      if (typeof options.destroy === "function") this._destroy = options.destroy
      if (typeof options.construct === "function") queueMicrotask(() => options.construct.call(this, (e) => e && this.destroy(e)))
    }
    get readableObjectMode() {
      return this[kRead].objectMode
    }
    get readableHighWaterMark() {
      return this[kRead].highWaterMark
    }
    get readableLength() {
      return this[kRead].buffer.reduce((n, c) => n + (this[kRead].objectMode ? 1 : c.length), 0)
    }
    get readableFlowing() {
      return this[kRead].flowing
    }
    get readableEncoding() {
      return this[kRead].encoding
    }
    _read() {}
    push(chunk, encoding) {
      const r = this[kRead]
      if (r.destroyed) return false
      if (chunk === null) {
        r.ended = true
        this.#maybeEnd()
        return false
      }
      let c = toChunk(this, chunk, encoding, r.objectMode)
      if (!r.objectMode && r.encoding && typeof c !== "string") c = c.toString(r.encoding)
      r.buffer.push(c)
      this.#schedule()
      return this.readableLength < r.highWaterMark
    }
    unshift(chunk) {
      this[kRead].buffer.unshift(chunk)
    }
    #schedule() {
      const r = this[kRead]
      if (r.scheduled) return
      r.scheduled = true
      queueMicrotask(() => {
        r.scheduled = false
        this.#flow()
      })
    }
    #flow() {
      const r = this[kRead]
      if (r.destroyed) return
      if (r.flowing) {
        while (r.flowing && r.buffer.length && !r.destroyed) this.emit("data", r.buffer.shift())
        if (r.flowing && !r.buffer.length && !r.ended && !r.destroyed) this.#callRead()
      } else if (r.buffer.length && this.listenerCount("readable") > 0) {
        this.emit("readable")
      }
      this.#maybeEnd()
    }
    #callRead() {
      const r = this[kRead]
      if (r.reading || r.ended || r.destroyed) return
      r.reading = true
      try {
        this._read(r.highWaterMark)
      } catch (e) {
        this.destroy(e)
      }
      r.reading = false
    }
    #maybeEnd() {
      const r = this[kRead]
      if (r.ended && r.buffer.length === 0 && !r.endEmitted && !r.destroyed) {
        r.endEmitted = true
        this.readable = false
        this.readableEnded = true
        queueMicrotask(() => {
          if (this.listenerCount("readable") > 0) this.emit("readable")
          this.emit("end")
          if (r.autoDestroy && !this[kWrite]) this.destroy()
          else if (r.autoDestroy && this[kWrite] && this[kWrite].finished) this.destroy()
        })
      }
    }
    on(event, fn) {
      super.on(event, fn)
      if (event === "data" && this[kRead].flowing !== false) this.resume()
      if (event === "readable") {
        this[kRead].flowing = false
        this.#schedule()
        if (!this[kRead].buffer.length) queueMicrotask(() => this.#callRead())
      }
      return this
    }
    resume() {
      const r = this[kRead]
      if (r.flowing !== true) {
        r.flowing = true
        this.#schedule()
      }
      return this
    }
    pause() {
      this[kRead].flowing = false
      return this
    }
    isPaused() {
      return this[kRead].flowing === false
    }
    read(size) {
      const r = this[kRead]
      if (!r.buffer.length && !r.ended) this.#callRead()
      if (!r.buffer.length) {
        this.#maybeEnd()
        return null
      }
      if (r.objectMode || size === undefined) {
        const c = r.buffer.shift()
        this.#maybeEnd()
        if (!r.buffer.length && !r.ended) queueMicrotask(() => this.#callRead())
        return c
      }
      let out = typeof r.buffer[0] === "string" ? r.buffer.join("") : Buffer.concat(r.buffer)
      r.buffer = []
      if (out.length > size) {
        r.buffer.push(out.slice(size))
        out = out.slice(0, size)
      }
      this.#maybeEnd()
      return out
    }
    setEncoding(encoding) {
      const r = this[kRead]
      r.encoding = encoding ?? "utf8"
      r.buffer = r.buffer.map((c) => (typeof c === "string" ? c : c.toString(r.encoding)))
      return this
    }
    pipe(dest, options) {
      const r = this[kRead]
      r.pipes.push(dest)
      const onData = (chunk) => {
        if (dest.write(chunk) === false && typeof dest.once === "function") {
          this.pause()
          dest.once("drain", () => this.resume())
        }
      }
      this.on("data", onData)
      if (options?.end !== false) this.once("end", () => dest.end())
      if (typeof dest.emit === "function") dest.emit("pipe", this)
      return dest
    }
    unpipe(dest) {
      const r = this[kRead]
      r.pipes = dest ? r.pipes.filter((d) => d !== dest) : []
      return this
    }
    destroy(err) {
      return destroyStream(this, err)
    }
    async *[Symbol.asyncIterator]() {
      const queue = []
      let done = false
      let error = null
      let wake = null
      this.on("data", (c) => {
        queue.push(c)
        if (wake) wake()
      })
      this.once("end", () => {
        done = true
        if (wake) wake()
      })
      this.once("error", (e) => {
        error = e
        if (wake) wake()
      })
      this.once("close", () => {
        done = true
        if (wake) wake()
      })
      try {
        while (true) {
          if (queue.length) {
            yield queue.shift()
            continue
          }
          if (error) throw error
          if (done) return
          await new Promise((resolve) => (wake = resolve))
          wake = null
        }
      } finally {
        if (!done && !error) this.destroy()
      }
    }
    async toArray() {
      const out = []
      for await (const c of this) out.push(c)
      return out
    }
    static from(iterable, options = {}) {
      const r = new Readable({ objectMode: true, ...options })
      const source = typeof iterable === "string" || iterable instanceof Uint8Array ? [iterable] : iterable
      ;(async () => {
        try {
          for await (const c of source) {
            if (r.destroyed) return
            r.push(c)
          }
          r.push(null)
        } catch (e) {
          r.destroy(e)
        }
      })()
      return r
    }
  }

  function initWritable(stream, options) {
    stream[kWrite] = {
      queue: [],
      writing: false,
      ending: false,
      finished: false,
      destroyed: false,
      needDrain: false,
      corked: 0,
      objectMode: !!(options.objectMode || options.writableObjectMode),
      highWaterMark: options.highWaterMark ?? options.writableHighWaterMark ?? 16384,
      decodeStrings: options.decodeStrings !== false,
      defaultEncoding: options.defaultEncoding ?? "utf8",
      autoDestroy: options.autoDestroy !== false,
    }
    stream.writable = true
    stream.writableEnded = false
    stream.writableFinished = false
    stream.destroyed = false
    if (typeof options.write === "function") stream._write = options.write
    if (typeof options.writev === "function") stream._writev = options.writev
    if (typeof options.final === "function") stream._final = options.final
    if (typeof options.destroy === "function") stream._destroy = options.destroy
  }
  const writableMethods = {
    _write(chunk, encoding, cb) {
      cb(streamError("ERR_METHOD_NOT_IMPLEMENTED", "The _write() method is not implemented"))
    },
    write(chunk, encoding, cb) {
      const w = this[kWrite]
      if (typeof encoding === "function") [encoding, cb] = [undefined, encoding]
      if (w.ending || w.destroyed) {
        const e = streamError("ERR_STREAM_WRITE_AFTER_END", "write after end")
        queueMicrotask(() => {
          if (typeof cb === "function") cb(e)
          this.emit("error", e)
        })
        return false
      }
      let enc = encoding ?? (typeof chunk === "string" ? w.defaultEncoding : "buffer")
      let c = chunk
      if (!w.objectMode) {
        if (typeof chunk === "string" && w.decodeStrings) {
          c = Buffer.from(chunk, enc)
          enc = "buffer"
        } else if (typeof chunk !== "string") {
          c = toChunk(this, chunk, undefined, false)
          enc = "buffer"
        }
      }
      w.queue.push([c, enc, cb])
      processWrites(this)
      const ok = w.queue.length < w.highWaterMark
      if (!ok) w.needDrain = true
      return ok
    },
    end(chunk, encoding, cb) {
      const w = this[kWrite]
      if (typeof chunk === "function") [chunk, encoding, cb] = [undefined, undefined, chunk]
      if (typeof encoding === "function") [encoding, cb] = [undefined, encoding]
      if (chunk != null) this.write(chunk, encoding)
      if (typeof cb === "function") {
        if (w.finished) queueMicrotask(cb)
        else this.once("finish", cb)
      }
      if (!w.ending) {
        w.ending = true
        this.writableEnded = true
        this.writable = false
        queueMicrotask(() => processWrites(this))
      }
      return this
    },
    cork() {
      this[kWrite].corked++
    },
    uncork() {
      const w = this[kWrite]
      if (w.corked > 0) w.corked--
      if (w.corked === 0) processWrites(this)
    },
    setDefaultEncoding(encoding) {
      this[kWrite].defaultEncoding = encoding
      return this
    },
    destroy(err) {
      return destroyStream(this, err)
    },
  }
  function processWrites(stream) {
    const w = stream[kWrite]
    if (w.writing || w.destroyed || w.corked > 0) return
    const next = w.queue.shift()
    if (!next) {
      if (w.ending && !w.finished) finishWritable(stream)
      else if (w.needDrain) {
        w.needDrain = false
        stream.emit("drain")
      }
      return
    }
    w.writing = true
    const [chunk, enc, cb] = next
    let called = false
    const done = (err) => {
      if (called) return
      called = true
      w.writing = false
      if (err) {
        if (typeof cb === "function") cb(err)
        stream.destroy(err)
        return
      }
      if (typeof cb === "function") cb(null)
      queueMicrotask(() => processWrites(stream))
    }
    try {
      stream._write(chunk, enc, done)
    } catch (e) {
      done(e)
    }
  }
  function finishWritable(stream) {
    const w = stream[kWrite]
    w.finished = true
    const fin = (err) => {
      if (err) return stream.destroy(err)
      stream.writableFinished = true
      stream.emit("finish")
      const r = stream[kRead]
      if (w.autoDestroy && (!r || r.endEmitted)) stream.destroy()
    }
    if (typeof stream._final === "function") {
      try {
        stream._final(fin)
      } catch (e) {
        fin(e)
      }
    } else fin()
  }

  class Writable extends EventEmitter {
    constructor(options = {}) {
      super()
      initWritable(this, options)
      if (typeof options.construct === "function") queueMicrotask(() => options.construct.call(this, (e) => e && this.destroy(e)))
    }
    get writableObjectMode() {
      return this[kWrite].objectMode
    }
    get writableHighWaterMark() {
      return this[kWrite].highWaterMark
    }
    get writableLength() {
      return this[kWrite].queue.length
    }
    get writableNeedDrain() {
      return this[kWrite].needDrain
    }
    get writableCorked() {
      return this[kWrite].corked
    }
  }
  Object.assign(Writable.prototype, writableMethods)

  class Duplex extends Readable {
    constructor(options = {}) {
      super({ ...options, read: options.read, objectMode: options.objectMode || options.readableObjectMode, highWaterMark: options.readableHighWaterMark ?? options.highWaterMark })
      initWritable(this, options)
      this.allowHalfOpen = options.allowHalfOpen !== false
    }
    get writableObjectMode() {
      return this[kWrite].objectMode
    }
    get writableHighWaterMark() {
      return this[kWrite].highWaterMark
    }
    get writableLength() {
      return this[kWrite].queue.length
    }
    get writableNeedDrain() {
      return this[kWrite].needDrain
    }
  }
  for (const name of Object.keys(writableMethods)) if (name !== "destroy") Duplex.prototype[name] = writableMethods[name]

  class Transform extends Duplex {
    constructor(options = {}) {
      super(options)
      if (typeof options.transform === "function") this._transform = options.transform
      if (typeof options.flush === "function") this._flush = options.flush
    }
    _transform(chunk, encoding, cb) {
      cb(streamError("ERR_METHOD_NOT_IMPLEMENTED", "The _transform() method is not implemented"))
    }
    _write(chunk, encoding, cb) {
      let called = false
      try {
        this._transform(chunk, encoding, (err, data) => {
          if (called) return
          called = true
          if (err) return cb(err)
          if (data != null) this.push(data)
          cb()
        })
      } catch (e) {
        cb(e)
      }
    }
    _final(cb) {
      const finish = (err, data) => {
        if (err) return cb(err)
        if (data != null) this.push(data)
        this.push(null)
        cb()
      }
      if (typeof this._flush === "function") {
        try {
          this._flush(finish)
        } catch (e) {
          cb(e)
        }
      } else finish()
    }
  }

  class PassThrough extends Transform {
    _transform(chunk, encoding, cb) {
      cb(null, chunk)
    }
  }

  function finished(stream, options, cb) {
    if (typeof options === "function") [options, cb] = [{}, options]
    let done = false
    const settle = (err) => {
      if (done) return
      done = true
      cleanup()
      cb(err)
    }
    const onEnd = () => settle()
    const onError = (e) => settle(e)
    const onClose = () => {
      if (stream[kRead] && !stream[kRead].endEmitted && !stream[kRead].destroyed) return settle(streamError("ERR_STREAM_PREMATURE_CLOSE", "Premature close"))
      if (stream[kWrite] && !stream[kWrite].finished) return settle(streamError("ERR_STREAM_PREMATURE_CLOSE", "Premature close"))
      settle()
    }
    const cleanup = () => {
      if (typeof stream.off !== "function") return
      stream.off("end", onEnd)
      stream.off("finish", onEnd)
      stream.off("error", onError)
      stream.off("close", onClose)
    }
    if (typeof stream.on !== "function") {
      queueMicrotask(() => settle())
      return cleanup
    }
    const readable = stream[kRead] !== undefined || typeof stream.read === "function" || stream.readable === true
    const writable = stream[kWrite] !== undefined || typeof stream.write === "function"
    if (readable && (!writable || options?.writable === false)) stream.on("end", onEnd)
    else stream.on("finish", onEnd)
    if (readable && writable && options?.writable !== false && options?.readable !== false) {
      let ended = false
      let finishedW = false
      stream.off("finish", onEnd)
      stream.on("end", () => {
        ended = true
        if (finishedW) settle()
      })
      stream.on("finish", () => {
        finishedW = true
        if (ended) settle()
      })
    }
    stream.on("error", onError)
    stream.on("close", onClose)
    return cleanup
  }

  function pipeline(...args) {
    const cb = typeof args[args.length - 1] === "function" ? args.pop() : null
    const streams = args.flat()
    if (streams.length < 2) throw streamError("ERR_MISSING_ARGS", "The streams argument must contain at least two streams")
    let settled = false
    const settle = (err) => {
      if (settled) return
      settled = true
      if (err) for (const s of streams) if (typeof s.destroy === "function" && !s.destroyed) s.destroy(err)
      if (cb) cb(err ?? undefined, streams[streams.length - 1][kRead] ? undefined : undefined)
    }
    for (let i = 0; i < streams.length; i++) {
      const s = streams[i]
      if (typeof s.on === "function") s.on("error", settle)
      if (i < streams.length - 1) {
        const next = streams[i + 1]
        if (typeof s.pipe === "function") s.pipe(next)
        else if (s && typeof s[Symbol.asyncIterator] === "function") Readable.from(s).pipe(next)
        else throw streamError("ERR_INVALID_ARG_TYPE", "pipeline expects streams")
      }
    }
    finished(streams[streams.length - 1], (err) => settle(err))
    return streams[streams.length - 1]
  }

  const streamPromises = {
    pipeline: (...streams) =>
      new Promise((resolve, reject) => {
        pipeline(...streams, (err) => (err ? reject(err) : resolve()))
      }),
    finished: (stream, options) =>
      new Promise((resolve, reject) => {
        finished(stream, options ?? {}, (err) => (err ? reject(err) : resolve()))
      }),
  }

  const stream = Object.assign(Readable, { Readable, Writable, Duplex, Transform, PassThrough, Stream: Readable, pipeline, finished, promises: streamPromises })
  stream.isReadable = (s) => !!(s && s[kRead] && !s[kRead].destroyed && !s[kRead].endEmitted)
  stream.isWritable = (s) => !!(s && s[kWrite] && !s[kWrite].destroyed && !s[kWrite].ending)
  stream.isErrored = () => false
  stream.addAbortSignal = (signal, s) => {
    signal.addEventListener("abort", () => s.destroy(new DOMException("The operation was aborted", "AbortError")), { once: true })
    return s
  }

  class IncomingResponse extends Readable {
    constructor(res) {
      super()
      this.statusCode = res.status
      this.statusMessage = res.statusText
      this.headers = {}
      this.rawHeaders = []
      for (const [k, v] of res.headers) {
        this.headers[k] = this.headers[k] === undefined ? v : `${this.headers[k]}, ${v}`
        this.rawHeaders.push(k, v)
      }
      this.httpVersion = "1.1"
      this.url = ""
      this.complete = false
      res.arrayBuffer().then(
        (buf) => {
          if (buf.byteLength) this.push(Buffer.from(buf))
          this.complete = true
          this.push(null)
        },
        (e) => this.destroy(e),
      )
    }
    text() {
      return this.toArray().then((parts) => Buffer.concat(parts.map((p) => (typeof p === "string" ? Buffer.from(p) : p))).toString("utf8"))
    }
    json() {
      return this.text().then((t) => JSON.parse(t))
    }
  }

  class ClientRequest extends Writable {
    #url
    #options
    #chunks = []
    #sent = false
    #controller = new AbortController()
    constructor(url, options, cb) {
      super({ highWaterMark: Infinity })
      if (typeof url === "string" || url instanceof URL) {
        this.#url = new URL(String(url))
        this.#options = options ?? {}
      } else {
        this.#options = url ?? {}
        const o = this.#options
        const protocol = o.protocol ?? "http:"
        const host = o.hostname ?? o.host ?? "localhost"
        const port = o.port ? `:${o.port}` : ""
        this.#url = new URL(`${protocol}//${host}${port}${o.path ?? "/"}`)
        cb = cb ?? options
      }
      if (typeof options === "function") cb = options
      this.method = (this.#options.method ?? "GET").toUpperCase()
      this.path = this.#url.pathname + this.#url.search
      this.host = this.#url.hostname
      this.protocol = this.#url.protocol
      this.aborted = false
      this.headersSent = false
      this.reusedSocket = false
      this.#headers = new Headers()
      for (const [k, v] of Object.entries(this.#options.headers ?? {})) this.setHeader(k, v)
      if (typeof cb === "function") this.once("response", cb)
      if (this.#options.timeout) this.setTimeout(this.#options.timeout)
      if (this.#options.signal) this.#options.signal.addEventListener("abort", () => this.destroy(this.#options.signal.reason), { once: true })
    }
    #headers
    setHeader(name, value) {
      this.#headers.delete(name)
      for (const v of Array.isArray(value) ? value : [value]) this.#headers.append(name, String(v))
      return this
    }
    getHeader(name) {
      return this.#headers.get(name) ?? undefined
    }
    getHeaders() {
      return Object.fromEntries(this.#headers)
    }
    removeHeader(name) {
      this.#headers.delete(name)
    }
    hasHeader(name) {
      return this.#headers.has(name)
    }
    setTimeout(ms, cb) {
      if (typeof cb === "function") this.once("timeout", cb)
      setTimeout(() => {
        if (!this.#sent || this.destroyed) return
        this.emit("timeout")
      }, ms)
      return this
    }
    setNoDelay() {}
    setSocketKeepAlive() {}
    flushHeaders() {}
    abort() {
      this.aborted = true
      this.#controller.abort()
      this.destroy()
    }
    _write(chunk, encoding, cb) {
      this.#chunks.push(chunk)
      cb()
    }
    _final(cb) {
      this.#sent = true
      this.headersSent = true
      const body = this.#chunks.length ? Buffer.concat(this.#chunks) : undefined
      fetch(this.#url, { method: this.method, headers: this.#headers, body, signal: this.#controller.signal, redirect: "follow" }).then(
        (res) => {
          const incoming = new IncomingResponse(res)
          incoming.req = this
          this.res = incoming
          this.emit("response", incoming)
          cb()
        },
        (e) => {
          cb()
          if (!this.destroyed) this.destroy(e)
        },
      )
    }
  }

  function httpRequest(url, options, cb) {
    return new ClientRequest(url, options, cb)
  }
  function httpGet(url, options, cb) {
    const req = httpRequest(url, options, cb)
    req.end()
    return req
  }
  class Agent {
    constructor(options = {}) {
      this.options = options
      this.maxSockets = options.maxSockets ?? Infinity
      this.keepAlive = !!options.keepAlive
    }
    destroy() {}
  }
  const httpClient = { request: httpRequest, get: httpGet, Agent, globalAgent: new Agent(), ClientRequest, IncomingMessage: IncomingResponse, METHODS: ["ACL", "BIND", "CHECKOUT", "CONNECT", "COPY", "DELETE", "GET", "HEAD", "LINK", "LOCK", "M-SEARCH", "MERGE", "MKACTIVITY", "MKCALENDAR", "MKCOL", "MOVE", "NOTIFY", "OPTIONS", "PATCH", "POST", "PROPFIND", "PROPPATCH", "PURGE", "PUT", "QUERY", "REBIND", "REPORT", "SEARCH", "SOURCE", "SUBSCRIBE", "TRACE", "UNBIND", "UNLINK", "UNLOCK", "UNSUBSCRIBE"] }

  const zlibConstants = {
    Z_NO_FLUSH: 0, Z_PARTIAL_FLUSH: 1, Z_SYNC_FLUSH: 2, Z_FULL_FLUSH: 3, Z_FINISH: 4, Z_BLOCK: 5,
    Z_OK: 0, Z_STREAM_END: 1, Z_NEED_DICT: 2, Z_ERRNO: -1, Z_STREAM_ERROR: -2, Z_DATA_ERROR: -3,
    Z_MEM_ERROR: -4, Z_BUF_ERROR: -5, Z_VERSION_ERROR: -6,
    Z_NO_COMPRESSION: 0, Z_BEST_SPEED: 1, Z_BEST_COMPRESSION: 9, Z_DEFAULT_COMPRESSION: -1,
    Z_FILTERED: 1, Z_HUFFMAN_ONLY: 2, Z_RLE: 3, Z_FIXED: 4, Z_DEFAULT_STRATEGY: 0,
    Z_MIN_LEVEL: -1, Z_MAX_LEVEL: 9, Z_DEFAULT_LEVEL: -1,
  }
  const zlibDecodes = new Set(["gunzip", "inflate", "inflateRaw", "unzip"])
  const zlibRun = (op, data, options) => {
    const level = typeof options === "number" ? options : options?.level ?? -1
    const input = typeof data === "string" ? Buffer.from(data) : cryptoBytes(data, undefined, "buffer")
    try {
      const out = native.zlib(op, input, level)
      return Buffer.from(out.buffer, out.byteOffset, out.byteLength)
    } catch (e) {
      const err = new Error(e?.message ?? String(e))
      err.code = zlibDecodes.has(op) ? "Z_DATA_ERROR" : "Z_STREAM_ERROR"
      err.errno = zlibDecodes.has(op) ? -3 : -2
      throw err
    }
  }
  const zlibAsync = (op) => (data, options, cb) => {
    if (typeof options === "function") [options, cb] = [undefined, options]
    if (typeof cb !== "function") throw new TypeError('The "callback" argument must be of type function')
    let out
    let err = null
    try {
      out = zlibRun(op, data, options)
    } catch (e) {
      err = e
    }
    queueMicrotask(() => (err ? cb(err) : cb(null, out)))
  }
  class ZlibStream extends Transform {
    #op
    #options
    #chunks = []
    constructor(op, options) {
      super({ highWaterMark: options?.highWaterMark })
      this.#op = op
      this.#options = options
      this.bytesWritten = 0
    }
    _transform(chunk, encoding, cb) {
      this.#chunks.push(chunk)
      this.bytesWritten += chunk.length
      cb()
    }
    _flush(cb) {
      try {
        cb(null, zlibRun(this.#op, Buffer.concat(this.#chunks), this.#options))
      } catch (e) {
        cb(e)
      }
    }
    flush(kind, cb) {
      if (typeof kind === "function") cb = kind
      if (typeof cb === "function") queueMicrotask(cb)
    }
    close(cb) {
      if (typeof cb === "function") this.once("close", cb)
      this.destroy()
    }
    params(level, strategy, cb) {
      if (typeof cb === "function") queueMicrotask(cb)
    }
    reset() {
      this.#chunks = []
    }
  }
  const zlib = {
    constants: zlibConstants,
    ...zlibConstants,
    crc32: (data, value = 0) => native.crc32(typeof data === "string" ? Buffer.from(data) : cryptoBytes(data, undefined, "data"), value),
  }
  for (const op of ["gzip", "gunzip", "deflate", "inflate", "deflateRaw", "inflateRaw", "unzip"]) {
    const Name = op[0].toUpperCase() + op.slice(1)
    zlib[op + "Sync"] = (data, options) => zlibRun(op, data, options)
    zlib[op] = zlibAsync(op)
    zlib["create" + Name] = (options) => new ZlibStream(op, options)
    zlib[Name] = class extends ZlibStream {
      constructor(options) {
        super(op, options)
      }
    }
    Object.defineProperty(zlib[Name], "name", { value: Name })
  }

  return {
    crypto,
    zlib,
    stream,
    streamPromises,
    httpClient,
    StringDecoder,
    querystring,
    tty,
    PerformanceObserver,
    Buffer,
    EventEmitter,
    fs,
    promises,
    fileURLToPath,
    pathToFileURL,
    assert,
    AssertionError,
    promisify,
    callbackify,
    deprecate,
    inherits,
    types,
    stdin,
    createInterface,
    Interface,
    readlinePromises,
    bytesFromString,
    stringFromBytes,
  }
}
"##;

pub fn load(vm: &mut Vm) -> Result<Value, Value> {
    if let Some(v) = &vm.node {
        return Ok(v.clone());
    }
    let dir = std::env::current_dir().unwrap_or_default();
    let proto = match vm.compile(NODE_SRC, "<squirt:node>", dir, Mode::Repl) {
        Ok(p) => p,
        Err(e) => return Err(vm.syntax_error_value(&e, "<squirt:node>")),
    };
    let factory = vm.run_proto(proto)?;
    let n = vm.new_object();
    let natives: [(&str, u32, crate::object::NativeFn); 27] = [
        ("read", 1, fs_read),
        ("write", 3, fs_write),
        ("exists", 1, fs_exists),
        ("access", 1, fs_access),
        ("stat", 2, fs_stat),
        ("readdir", 2, fs_readdir),
        ("mkdir", 2, fs_mkdir),
        ("mkdtemp", 1, fs_mkdtemp),
        ("rm", 3, fs_rm),
        ("rmdir", 2, fs_rmdir),
        ("unlink", 1, fs_unlink),
        ("rename", 2, fs_rename),
        ("copy", 2, fs_copy),
        ("cp", 3, fs_cp),
        ("realpath", 1, fs_realpath),
        ("symlink", 2, fs_symlink),
        ("readlink", 1, fs_readlink),
        ("resolve", 1, fs_resolve),
        ("inspect", 1, native_inspect),
        ("isTTY", 1, is_tty),
        ("stdin", 1, stdin_start),
        ("stdinStop", 0, stdin_stop),
        ("digest", 2, crypto_digest),
        ("hmac", 3, crypto_hmac),
        ("randomBytes", 1, crypto_random_bytes),
        ("zlib", 3, zlib_op),
        ("crc32", 2, zlib_crc32),
    ];
    for (name, arity, f) in natives {
        method(vm, &n, name, arity, f);
    }
    let exports = vm.call_value(&factory, Value::Undefined, &[Value::Object(n)])?;
    let buffer = vm.get_prop(&exports, "Buffer")?;
    vm.define_global("Buffer", buffer);
    vm.node = Some(exports.clone());
    Ok(exports)
}

fn pick(vm: &mut Vm, src: &Value, names: &[&str]) -> Result<Value, Value> {
    let o = vm.new_object();
    for name in names {
        let v = vm.get_prop(src, name)?;
        o.set_own(*name, v);
    }
    o.set_own("default", Value::Object(o.clone()));
    Ok(Value::Object(o))
}

fn with_default(vm: &mut Vm, v: Value) -> Result<Value, Value> {
    let Value::Object(o) = &v else {
        return Ok(v);
    };
    if o.get_own("default").is_none() {
        let copy = vm.new_object();
        for (k, x) in vm.own_entries(&v) {
            copy.set_own(k, x);
        }
        copy.set_own("default", v.clone());
        return Ok(Value::Object(copy));
    }
    Ok(v)
}

pub fn module(vm: &mut Vm, name: &str) -> Option<Result<Value, Value>> {
    let r = (|| -> Result<Option<Value>, Value> {
        Ok(Some(match name {
            "fs" => {
                let n = load(vm)?;
                let fs = vm.get_prop(&n, "fs")?;
                with_default(vm, fs)?
            }
            "fs/promises" => {
                let n = load(vm)?;
                let p = vm.get_prop(&n, "promises")?;
                with_default(vm, p)?
            }
            "path" | "path/posix" | "path/win32" => {
                let p = vm.import_builtin("path")?;
                match name {
                    "path/posix" => vm.get_prop(&p, "posix")?,
                    "path/win32" => vm.get_prop(&p, "win32")?,
                    _ => p,
                }
            }
            "os" | "child_process" => vm.import_builtin(name)?,
            "http" | "https" => {
                let h = vm.import_builtin("http")?;
                let n = load(vm)?;
                let client = vm.get_prop(&n, "httpClient")?;
                if let Value::Object(h) = &h {
                    for (k, v) in vm.own_entries(&client) {
                        if h.get_own(&k).is_none() {
                            h.set_own(k, v);
                        }
                    }
                    h.set_own("default", Value::Object(h.clone()));
                }
                h
            }
            "stream" => {
                let n = load(vm)?;
                let s = vm.get_prop(&n, "stream")?;
                if let Value::Object(o) = &s {
                    o.set_own("default", s.clone());
                }
                s
            }
            "stream/promises" => {
                let n = load(vm)?;
                let p = vm.get_prop(&n, "streamPromises")?;
                with_default(vm, p)?
            }
            "zlib" => {
                let n = load(vm)?;
                let z = vm.get_prop(&n, "zlib")?;
                with_default(vm, z)?
            }
            "events" => {
                let n = load(vm)?;
                let ee = vm.get_prop(&n, "EventEmitter")?;
                if let Value::Object(c) = &ee {
                    c.set_hidden("EventEmitter", ee.clone());
                    c.set_hidden("default", ee.clone());
                    let once = vm.get_prop(&ee, "once")?;
                    c.set_hidden("once", once);
                }
                ee
            }
            "buffer" => {
                let n = load(vm)?;
                pick(vm, &n, &["Buffer"])?
            }
            "assert" | "assert/strict" => {
                let n = load(vm)?;
                let a = vm.get_prop(&n, "assert")?;
                if let Value::Object(f) = &a {
                    f.set_hidden("default", a.clone());
                }
                a
            }
            "util" => {
                let n = load(vm)?;
                let base = vm.import_builtin("util")?;
                let o = vm.new_object();
                for (k, x) in vm.own_entries(&base) {
                    if &*k != "default" {
                        o.set_own(k, x);
                    }
                }
                for key in ["promisify", "callbackify", "deprecate", "inherits", "types"] {
                    let v = vm.get_prop(&n, key)?;
                    o.set_own(key, v);
                }
                let te = vm.globals.get("TextEncoder").cloned().unwrap_or_default();
                let td = vm.globals.get("TextDecoder").cloned().unwrap_or_default();
                o.set_own("TextEncoder", te);
                o.set_own("TextDecoder", td);
                o.set_own("default", Value::Object(o.clone()));
                Value::Object(o)
            }
            "url" => {
                let web = super::web::load(vm)?;
                let n = load(vm)?;
                let o = vm.new_object();
                for key in ["URL", "URLSearchParams"] {
                    let v = vm.get_prop(&web, key)?;
                    o.set_own(key, v);
                }
                for key in ["fileURLToPath", "pathToFileURL"] {
                    let v = vm.get_prop(&n, key)?;
                    o.set_own(key, v);
                }
                o.set_own("default", Value::Object(o.clone()));
                Value::Object(o)
            }
            "readline" => {
                let n = load(vm)?;
                let o = vm.new_object();
                for key in ["createInterface", "Interface"] {
                    let v = vm.get_prop(&n, key)?;
                    o.set_own(key, v);
                }
                let p = vm.get_prop(&n, "readlinePromises")?;
                o.set_own("promises", p);
                o.set_own("default", Value::Object(o.clone()));
                Value::Object(o)
            }
            "readline/promises" => {
                let n = load(vm)?;
                let p = vm.get_prop(&n, "readlinePromises")?;
                with_default(vm, p)?
            }
            "process" => {
                let p = vm.globals.get("process").cloned().unwrap_or_default();
                with_default(vm, p)?
            }
            "timers" => {
                let o = vm.new_object();
                for key in ["setTimeout", "setInterval", "clearTimeout", "clearInterval"] {
                    let v = vm.globals.get(key).cloned().unwrap_or_default();
                    o.set_own(key, v);
                }
                o.set_own("default", Value::Object(o.clone()));
                Value::Object(o)
            }
            "crypto" => {
                let n = load(vm)?;
                let c = vm.get_prop(&n, "crypto")?;
                with_default(vm, c)?
            }
            "string_decoder" => {
                let n = load(vm)?;
                pick(vm, &n, &["StringDecoder"])?
            }
            "querystring" => {
                let n = load(vm)?;
                let q = vm.get_prop(&n, "querystring")?;
                with_default(vm, q)?
            }
            "tty" => {
                let n = load(vm)?;
                let t = vm.get_prop(&n, "tty")?;
                with_default(vm, t)?
            }
            "perf_hooks" => {
                let n = load(vm)?;
                let o = vm.new_object();
                let p = vm.globals.get("performance").cloned().unwrap_or_default();
                o.set_own("performance", p);
                let po = vm.get_prop(&n, "PerformanceObserver")?;
                o.set_own("PerformanceObserver", po);
                o.set_own("default", Value::Object(o.clone()));
                Value::Object(o)
            }
            "timers/promises" => {
                let o = vm.new_object();
                let f = native_fn(vm, "setTimeout", 2, timers_set_timeout);
                o.set_own("setTimeout", f);
                o.set_own("default", Value::Object(o.clone()));
                Value::Object(o)
            }
            _ => return Ok(None),
        }))
    })();
    match r {
        Ok(Some(v)) => Some(Ok(v)),
        Ok(None) => None,
        Err(e) => Some(Err(e)),
    }
}

fn crypto_input(vm: &mut Vm, v: &Value, what: &str) -> Result<Vec<u8>, Value> {
    super::typed::buffer_source_bytes(v).ok_or_else(|| {
        vm.type_error(format!(
            "The \"{what}\" argument must be of type string or an instance of Buffer, TypedArray, or DataView"
        ))
    })
}

fn crypto_digest(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let alg = vm.to_string(&arg(args, 0))?;
    let data = crypto_input(vm, &arg(args, 1), "data")?;
    match super::binary::digest_bytes(&alg, &data) {
        Some(out) => Ok(super::typed::new_uint8(vm, out)),
        None => Err(vm.make_error(
            ErrorKind::Error,
            format!("Digest method not supported: {alg}"),
        )),
    }
}

fn crypto_hmac(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let alg = vm.to_string(&arg(args, 0))?;
    let key = crypto_input(vm, &arg(args, 1), "key")?;
    let data = crypto_input(vm, &arg(args, 2), "data")?;
    match super::binary::hmac_bytes(&alg, &key, &data) {
        Some(out) => Ok(super::typed::new_uint8(vm, out)),
        None => Err(vm.make_error(
            ErrorKind::Error,
            format!("Digest method not supported: {alg}"),
        )),
    }
}

fn crypto_random_bytes(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let n = vm.to_number(&arg(args, 0))?;
    if !(0.0..=2147483647.0).contains(&n) || n.fract() != 0.0 {
        return Err(vm.range_error(
            "The value of \"size\" is out of range. It must be >= 0 && <= 2147483647",
        ));
    }
    Ok(super::typed::new_uint8(
        vm,
        super::binary::random_vec(n as usize),
    ))
}

fn zlib_bytes(op: &str, data: &[u8], level: i32) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Write};
    let level = if level < 0 {
        flate2::Compression::default()
    } else {
        flate2::Compression::new(level.min(9) as u32)
    };
    match op {
        "gzip" => {
            let mut e = flate2::write::GzEncoder::new(Vec::new(), level);
            e.write_all(data)?;
            e.finish()
        }
        "deflate" => {
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), level);
            e.write_all(data)?;
            e.finish()
        }
        "deflateRaw" => {
            let mut e = flate2::write::DeflateEncoder::new(Vec::new(), level);
            e.write_all(data)?;
            e.finish()
        }
        "gunzip" => {
            let mut out = Vec::new();
            flate2::read::MultiGzDecoder::new(data).read_to_end(&mut out)?;
            Ok(out)
        }
        "inflate" => {
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(data).read_to_end(&mut out)?;
            Ok(out)
        }
        "inflateRaw" => {
            let mut out = Vec::new();
            flate2::read::DeflateDecoder::new(data).read_to_end(&mut out)?;
            Ok(out)
        }
        "unzip" => {
            if data.starts_with(&[0x1f, 0x8b]) {
                zlib_bytes("gunzip", data, -1)
            } else {
                zlib_bytes("inflate", data, -1)
            }
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown zlib operation {op}"),
        )),
    }
}

fn zlib_op(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let op = vm.to_string(&arg(args, 0))?;
    let data = crypto_input(vm, &arg(args, 1), "buffer")?;
    let level = match arg(args, 2) {
        Value::Number(n) if n.is_finite() => n as i32,
        _ => -1,
    };
    match zlib_bytes(&op, &data, level) {
        Ok(out) => Ok(super::typed::new_uint8(vm, out)),
        Err(e) => {
            let msg = match e.kind() {
                std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidData => {
                    "incorrect header check".to_string()
                }
                std::io::ErrorKind::UnexpectedEof => "unexpected end of file".to_string(),
                _ => e.to_string(),
            };
            Err(vm.make_error(ErrorKind::Error, msg))
        }
    }
}

fn zlib_crc32(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let data = crypto_input(vm, &arg(args, 0), "data")?;
    let init = match arg(args, 1) {
        Value::Number(n) if n.is_finite() => n as u32,
        _ => 0,
    };
    let mut h = crc32fast::Hasher::new_with_initial(init);
    h.update(&data);
    Ok(Value::Number(h.finalize() as f64))
}

fn timers_set_timeout(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let ms = vm.to_number(&arg(args, 0))?;
    let value = arg(args, 1);
    let p = vm.new_promise();
    let (resolve, _) = vm.resolving_functions(&p);
    vm.add_timer(resolve, ms, vec![value], false);
    Ok(Value::Object(p))
}

fn native_inspect(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(crate::inspect::inspect_plain(&arg(args, 0)).into())
}

fn is_tty(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    use std::io::IsTerminal;
    let fd = match arg(args, 0) {
        Value::Number(n) => n as i32,
        _ => 0,
    };
    Ok(Value::Bool(match fd {
        0 => std::io::stdin().is_terminal(),
        1 => std::io::stdout().is_terminal(),
        _ => std::io::stderr().is_terminal(),
    }))
}

fn error_code(e: &std::io::Error) -> (&'static str, &'static str, i32) {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::NotFound => ("ENOENT", "no such file or directory", -2),
        K::PermissionDenied => ("EACCES", "permission denied", -13),
        K::AlreadyExists => ("EEXIST", "file already exists", -17),
        K::DirectoryNotEmpty => ("ENOTEMPTY", "directory not empty", -39),
        K::IsADirectory => ("EISDIR", "illegal operation on a directory", -21),
        K::NotADirectory => ("ENOTDIR", "not a directory", -20),
        K::InvalidInput => ("EINVAL", "invalid argument", -22),
        _ => match e.raw_os_error() {
            Some(267) => ("ENOTDIR", "not a directory", -20),
            Some(145) => ("ENOTEMPTY", "directory not empty", -39),
            _ => ("EIO", "i/o error", -5),
        },
    }
}

pub fn fs_error(
    vm: &mut Vm,
    e: &std::io::Error,
    syscall: &str,
    path: &str,
    dest: Option<&str>,
) -> Value {
    let (code, text, errno) = error_code(e);
    let msg = match dest {
        Some(d) => format!("{code}: {text}, {syscall} '{path}' -> '{d}'"),
        None => format!("{code}: {text}, {syscall} '{path}'"),
    };
    let err = vm.make_error(ErrorKind::Error, msg);
    if let Value::Object(o) = &err {
        o.set_own("errno", Value::Number(errno as f64));
        o.set_own("code", code.into());
        o.set_own("syscall", syscall.into());
        o.set_own("path", path.into());
        if let Some(d) = dest {
            o.set_own("dest", d.into());
        }
    }
    err
}

fn path_of(vm: &mut Vm, args: &[Value], i: usize) -> Result<String, Value> {
    Ok(vm.to_string(&arg(args, i))?.to_string())
}

fn fs_read(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    if let Value::Number(fd) = arg(args, 0) {
        if fd == 0.0 {
            vm.flush();
            let mut buf = Vec::new();
            std::io::stdin()
                .read_to_end(&mut buf)
                .map_err(|e| fs_error(vm, &e, "read", "<stdin>", None))?;
            return Ok(new_uint8(vm, buf));
        }
    }
    let p = path_of(vm, args, 0)?;
    if Path::new(&p).is_dir() {
        let e = std::io::Error::new(std::io::ErrorKind::IsADirectory, "is a directory");
        return Err(fs_error(vm, &e, "read", &p, None));
    }
    match std::fs::read(&p) {
        Ok(b) => Ok(new_uint8(vm, b)),
        Err(e) => Err(fs_error(vm, &e, "open", &p, None)),
    }
}

fn fs_write(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    use std::io::Write;
    let p = path_of(vm, args, 0)?;
    let data = arg(args, 1);
    let bytes = match buffer_source_bytes(&data) {
        Some(b) => b,
        None => vm.to_string(&data)?.as_bytes().to_vec(),
    };
    let append = arg(args, 2).truthy();
    let r = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(&p)
        .and_then(|mut f| f.write_all(&bytes));
    r.map(|_| Value::Undefined)
        .map_err(|e| fs_error(vm, &e, "open", &p, None))
}

fn fs_exists(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_of(vm, args, 0)?;
    Ok(Value::Bool(Path::new(&p).exists()))
}

fn fs_access(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_of(vm, args, 0)?;
    std::fs::metadata(&p)
        .map(|_| Value::Undefined)
        .map_err(|e| fs_error(vm, &e, "access", &p, None))
}

fn millis(t: std::io::Result<std::time::SystemTime>) -> f64 {
    t.ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

fn kind_name(ft: std::fs::FileType) -> &'static str {
    if ft.is_symlink() {
        "symlink"
    } else if ft.is_dir() {
        "dir"
    } else if ft.is_file() {
        "file"
    } else {
        "other"
    }
}

fn fs_stat(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_of(vm, args, 0)?;
    let follow = arg(args, 1).truthy();
    let md = if follow {
        std::fs::metadata(&p)
    } else {
        std::fs::symlink_metadata(&p)
    };
    let md = md.map_err(|e| fs_error(vm, &e, if follow { "stat" } else { "lstat" }, &p, None))?;
    let o = vm.new_object();
    let kind = kind_name(md.file_type());
    let mode = match kind {
        "dir" => 0o40755,
        "symlink" => 0o120777,
        _ => {
            if md.permissions().readonly() {
                0o100444
            } else {
                0o100644
            }
        }
    };
    o.set_own("mode", Value::Number(mode as f64));
    o.set_own("size", Value::Number(md.len() as f64));
    o.set_own("atimeMs", Value::Number(millis(md.accessed())));
    let mtime = millis(md.modified());
    o.set_own("mtimeMs", Value::Number(mtime));
    o.set_own("ctimeMs", Value::Number(mtime));
    let birth = millis(md.created());
    o.set_own(
        "birthtimeMs",
        Value::Number(if birth == 0.0 { mtime } else { birth }),
    );
    o.set_own("kind", kind.into());
    Ok(Value::Object(o))
}

fn list_dir(
    vm: &mut Vm,
    root: &Path,
    rel: &str,
    recursive: bool,
    out: &mut Vec<Value>,
) -> Result<(), Value> {
    let dir = if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    let entries = std::fs::read_dir(&dir)
        .map_err(|e| fs_error(vm, &e, "scandir", &dir.display().to_string(), None))?;
    let mut items: Vec<(String, &'static str)> = entries
        .flatten()
        .map(|e| {
            let kind = e.file_type().map(kind_name).unwrap_or("other");
            (e.file_name().to_string_lossy().into_owned(), kind)
        })
        .collect();
    items.sort();
    let parent = dir.display().to_string();
    for (name, kind) in items {
        let path = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}{}{name}", std::path::MAIN_SEPARATOR)
        };
        out.push(vm.new_array(vec![
            name.as_str().into(),
            kind.into(),
            parent.as_str().into(),
            path.as_str().into(),
        ]));
        if recursive && kind == "dir" {
            list_dir(vm, root, &path, true, out)?;
        }
    }
    Ok(())
}

fn fs_readdir(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_of(vm, args, 0)?;
    let recursive = arg(args, 1).truthy();
    let mut out = Vec::new();
    list_dir(vm, Path::new(&p), "", recursive, &mut out)?;
    Ok(vm.new_array(out))
}

fn fs_mkdir(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_of(vm, args, 0)?;
    if arg(args, 1).truthy() {
        let first = Path::new(&p)
            .ancestors()
            .filter(|a| !a.as_os_str().is_empty() && !a.exists())
            .last()
            .map(|a| a.display().to_string());
        std::fs::create_dir_all(&p).map_err(|e| fs_error(vm, &e, "mkdir", &p, None))?;
        return Ok(first
            .and_then(|f| std::fs::canonicalize(&f).ok().map(|c| strip_unc(&c)))
            .map(Value::from)
            .unwrap_or_default());
    }
    std::fs::create_dir(&p)
        .map(|_| Value::Undefined)
        .map_err(|e| fs_error(vm, &e, "mkdir", &p, None))
}

fn strip_unc(p: &Path) -> String {
    let s = p.display().to_string();
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

fn fs_mkdtemp(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let prefix = path_of(vm, args, 0)?;
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    for _ in 0..100 {
        let mut suffix = String::new();
        for _ in 0..6 {
            let i = (vm.random() * CHARS.len() as f64) as usize % CHARS.len();
            suffix.push(CHARS[i] as char);
        }
        let p = format!("{prefix}{suffix}");
        match std::fs::create_dir(&p) {
            Ok(()) => return Ok(p.into()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(fs_error(
                    vm,
                    &e,
                    "mkdtemp",
                    &format!("{prefix}XXXXXX"),
                    None,
                ));
            }
        }
    }
    let e = std::io::Error::new(std::io::ErrorKind::AlreadyExists, "exists");
    Err(fs_error(vm, &e, "mkdtemp", &prefix, None))
}

fn fs_rm(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_of(vm, args, 0)?;
    let recursive = arg(args, 1).truthy();
    let force = arg(args, 2).truthy();
    let path = Path::new(&p);
    let md = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => {
            if force && e.kind() == std::io::ErrorKind::NotFound {
                return Ok(Value::Undefined);
            }
            return Err(fs_error(vm, &e, "rm", &p, None));
        }
    };
    let r = if md.is_dir() {
        if !recursive {
            let err = vm.make_error(
                ErrorKind::Error,
                format!("Path is a directory: rm returned EISDIR (is a directory) {p}"),
            );
            if let Value::Object(o) = &err {
                o.set_own("code", "ERR_FS_EISDIR".into());
            }
            return Err(err);
        }
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    r.map(|_| Value::Undefined)
        .map_err(|e| fs_error(vm, &e, "rm", &p, None))
}

fn fs_rmdir(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_of(vm, args, 0)?;
    let r = if arg(args, 1).truthy() {
        std::fs::remove_dir_all(&p)
    } else {
        std::fs::remove_dir(&p)
    };
    r.map(|_| Value::Undefined)
        .map_err(|e| fs_error(vm, &e, "rmdir", &p, None))
}

fn fs_unlink(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_of(vm, args, 0)?;
    std::fs::remove_file(&p)
        .map(|_| Value::Undefined)
        .map_err(|e| fs_error(vm, &e, "unlink", &p, None))
}

fn fs_rename(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let a = path_of(vm, args, 0)?;
    let b = path_of(vm, args, 1)?;
    std::fs::rename(&a, &b)
        .map(|_| Value::Undefined)
        .map_err(|e| fs_error(vm, &e, "rename", &a, Some(&b)))
}

fn fs_copy(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let a = path_of(vm, args, 0)?;
    let b = path_of(vm, args, 1)?;
    std::fs::copy(&a, &b)
        .map(|_| Value::Undefined)
        .map_err(|e| fs_error(vm, &e, "copyfile", &a, Some(&b)))
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

fn fs_cp(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let a = path_of(vm, args, 0)?;
    let b = path_of(vm, args, 1)?;
    if Path::new(&a).is_dir() && !arg(args, 2).truthy() {
        let err = vm.make_error(
            ErrorKind::Error,
            format!("Recursive option is required to copy a directory: cp returned EISDIR ({a} is a directory (not copied)) {a}"),
        );
        if let Value::Object(o) = &err {
            o.set_own("code", "ERR_FS_EISDIR".into());
        }
        return Err(err);
    }
    copy_tree(Path::new(&a), Path::new(&b))
        .map(|_| Value::Undefined)
        .map_err(|e| fs_error(vm, &e, "cp", &a, Some(&b)))
}

fn fs_realpath(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_of(vm, args, 0)?;
    std::fs::canonicalize(&p)
        .map(|c| Value::from(strip_unc(&c)))
        .map_err(|e| fs_error(vm, &e, "realpath", &p, None))
}

fn fs_symlink(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = path_of(vm, args, 0)?;
    let link = path_of(vm, args, 1)?;
    #[cfg(unix)]
    let r = std::os::unix::fs::symlink(&target, &link);
    #[cfg(windows)]
    let r = if Path::new(&target).is_dir() {
        std::os::windows::fs::symlink_dir(&target, &link)
    } else {
        std::os::windows::fs::symlink_file(&target, &link)
    };
    #[cfg(not(any(unix, windows)))]
    let r: std::io::Result<()> = Err(std::io::Error::other("unsupported"));
    r.map(|_| Value::Undefined)
        .map_err(|e| fs_error(vm, &e, "symlink", &target, Some(&link)))
}

fn fs_readlink(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_of(vm, args, 0)?;
    std::fs::read_link(&p)
        .map(|t| Value::from(t.display().to_string()))
        .map_err(|e| fs_error(vm, &e, "readlink", &p, None))
}

fn fs_resolve(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_of(vm, args, 0)?;
    let path = Path::new(&p);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    Ok(strip_unc(&abs).into())
}

fn stdin_start(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    if vm.stdin_handler.is_some() {
        return Ok(Value::Undefined);
    }
    vm.stdin_handler = Some(arg(args, 0));
    vm.io.pending += 1;
    let tx = vm.io.tx.clone();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(IoEvent::Stdin(None));
                    break;
                }
                Ok(n) => {
                    if tx.send(IoEvent::Stdin(Some(buf[..n].to_vec()))).is_err() {
                        break;
                    }
                }
            }
        }
    });
    Ok(Value::Undefined)
}

fn stdin_stop(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    if !vm.stdin_stopped && vm.stdin_handler.is_some() {
        vm.stdin_stopped = true;
        vm.io.pending -= 1;
    }
    Ok(Value::Undefined)
}

pub fn handle_stdin(vm: &mut Vm, chunk: Option<Vec<u8>>) -> Result<(), Value> {
    let Some(handler) = vm.stdin_handler.clone() else {
        return Ok(());
    };
    let stopped = vm.stdin_stopped;
    let arg = match chunk {
        Some(bytes) => {
            if stopped {
                return Ok(());
            }
            new_uint8(vm, bytes)
        }
        None => {
            if !stopped {
                vm.stdin_stopped = true;
                vm.io.pending -= 1;
            }
            Value::Null
        }
    };
    vm.call_value(&handler, Value::Undefined, &[arg])?;
    Ok(())
}
