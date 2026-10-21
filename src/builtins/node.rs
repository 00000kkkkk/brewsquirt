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

  return {
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
    let natives: [(&str, u32, crate::object::NativeFn); 22] = [
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
            "http" => vm.import_builtin("http")?,
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
