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
  for (const key of ["from", "alloc", "allocUnsafe", "allocUnsafeSlow", "byteLength", "concat", "isBuffer", "isEncoding", "compare"]) {
    Object.defineProperty(Buffer, key, { value: Buffer[key], writable: true, enumerable: true, configurable: true })
  }
  Buffer.poolSize = 8192
  const bufferView = (b) => new DataView(b.buffer, b.byteOffset, b.byteLength)
  Object.assign(Buffer.prototype, {
    copy(target, targetStart = 0, sourceStart = 0, sourceEnd = this.length) {
      sourceEnd = Math.min(sourceEnd, this.length)
      const n = Math.max(0, Math.min(sourceEnd - sourceStart, target.length - targetStart))
      if (n > 0) target.set(this.subarray(sourceStart, sourceStart + n), targetStart)
      return n
    },
    lastIndexOf(value, byteOffset = this.length - 1, encoding) {
      if (typeof byteOffset === "string") [byteOffset, encoding] = [this.length - 1, byteOffset]
      const needle = typeof value === "number" ? Uint8Array.of(value & 255) : typeof value === "string" ? Buffer.from(value, encoding) : value
      if (byteOffset < 0) byteOffset += this.length
      if (needle.length === 0) return Math.min(byteOffset + 1, this.length)
      for (let i = Math.min(byteOffset, this.length - needle.length); i >= 0; i--) {
        let ok = true
        for (let j = 0; j < needle.length; j++) {
          if (this[i + j] !== needle[j]) {
            ok = false
            break
          }
        }
        if (ok) return i
      }
      return -1
    },
    swap16() {
      if (this.length % 2 !== 0) throw new RangeError("Buffer size must be a multiple of 16-bits")
      for (let i = 0; i < this.length; i += 2) [this[i], this[i + 1]] = [this[i + 1], this[i]]
      return this
    },
    swap32() {
      if (this.length % 4 !== 0) throw new RangeError("Buffer size must be a multiple of 32-bits")
      for (let i = 0; i < this.length; i += 4) [this[i], this[i + 1], this[i + 2], this[i + 3]] = [this[i + 3], this[i + 2], this[i + 1], this[i]]
      return this
    },
    swap64() {
      if (this.length % 8 !== 0) throw new RangeError("Buffer size must be a multiple of 64-bits")
      for (let i = 0; i < this.length; i += 8) this.subarray(i, i + 8).reverse()
      return this
    },
    readBigInt64LE(offset = 0) {
      return bufferView(this).getBigInt64(offset, true)
    },
    readBigInt64BE(offset = 0) {
      return bufferView(this).getBigInt64(offset, false)
    },
    readBigUInt64LE(offset = 0) {
      return bufferView(this).getBigUint64(offset, true)
    },
    readBigUInt64BE(offset = 0) {
      return bufferView(this).getBigUint64(offset, false)
    },
    writeBigInt64LE(value, offset = 0) {
      bufferView(this).setBigInt64(offset, value, true)
      return offset + 8
    },
    writeBigInt64BE(value, offset = 0) {
      bufferView(this).setBigInt64(offset, value, false)
      return offset + 8
    },
    writeBigUInt64LE(value, offset = 0) {
      bufferView(this).setBigUint64(offset, value, true)
      return offset + 8
    },
    writeBigUInt64BE(value, offset = 0) {
      bufferView(this).setBigUint64(offset, value, false)
      return offset + 8
    },
    readUIntLE(offset = 0, byteLength = 1) {
      let v = 0
      let mul = 1
      for (let i = 0; i < byteLength; i++) {
        v += this[offset + i] * mul
        mul *= 256
      }
      return v
    },
    readUIntBE(offset = 0, byteLength = 1) {
      let v = 0
      for (let i = 0; i < byteLength; i++) v = v * 256 + this[offset + i]
      return v
    },
    readIntLE(offset = 0, byteLength = 1) {
      const v = this.readUIntLE(offset, byteLength)
      const limit = 2 ** (8 * byteLength - 1)
      return v >= limit ? v - 2 * limit : v
    },
    readIntBE(offset = 0, byteLength = 1) {
      const v = this.readUIntBE(offset, byteLength)
      const limit = 2 ** (8 * byteLength - 1)
      return v >= limit ? v - 2 * limit : v
    },
    writeUIntLE(value, offset = 0, byteLength = 1) {
      let v = value
      for (let i = 0; i < byteLength; i++) {
        this[offset + i] = v % 256
        v = Math.floor(v / 256)
      }
      return offset + byteLength
    },
    writeUIntBE(value, offset = 0, byteLength = 1) {
      let v = value
      for (let i = byteLength - 1; i >= 0; i--) {
        this[offset + i] = v % 256
        v = Math.floor(v / 256)
      }
      return offset + byteLength
    },
    writeIntLE(value, offset = 0, byteLength = 1) {
      const limit = 2 ** (8 * byteLength)
      return this.writeUIntLE(value < 0 ? value + limit : value, offset, byteLength)
    },
    writeIntBE(value, offset = 0, byteLength = 1) {
      const limit = 2 ** (8 * byteLength)
      return this.writeUIntBE(value < 0 ? value + limit : value, offset, byteLength)
    },
    toLocaleString(encoding, start, end) {
      return this.toString(encoding, start, end)
    },
    inspect() {
      const shown = Array.from(this.subarray(0, 50), (b) => b.toString(16).padStart(2, "0")).join(" ")
      return `<Buffer ${shown}${this.length > 50 ? ` ... ${this.length - 50} more bytes` : ""}>`
    },
    hexSlice(start, end) {
      return this.toString("hex", start, end)
    },
    utf8Slice(start, end) {
      return this.toString("utf8", start, end)
    },
    latin1Slice(start, end) {
      return this.toString("latin1", start, end)
    },
    asciiSlice(start, end) {
      return this.toString("ascii", start, end)
    },
    base64Slice(start, end) {
      return this.toString("base64", start, end)
    },
    ucs2Slice(start, end) {
      return this.toString("ucs2", start, end)
    },
    hexWrite(string, offset, length) {
      return this.write(string, offset, length, "hex")
    },
    utf8Write(string, offset, length) {
      return this.write(string, offset, length, "utf8")
    },
    latin1Write(string, offset, length) {
      return this.write(string, offset, length, "latin1")
    },
    asciiWrite(string, offset, length) {
      return this.write(string, offset, length, "ascii")
    },
    base64Write(string, offset, length) {
      return this.write(string, offset, length, "base64")
    },
    ucs2Write(string, offset, length) {
      return this.write(string, offset, length, "ucs2")
    },
  })
  for (const [alias, name] of [["readBigUint64LE", "readBigUInt64LE"], ["readBigUint64BE", "readBigUInt64BE"], ["writeBigUint64LE", "writeBigUInt64LE"], ["writeBigUint64BE", "writeBigUInt64BE"], ["readUintLE", "readUIntLE"], ["readUintBE", "readUIntBE"], ["writeUintLE", "writeUIntLE"], ["writeUintBE", "writeUIntBE"]]) {
    Buffer.prototype[alias] = Buffer.prototype[name]
  }
  Object.defineProperty(Buffer.prototype, "parent", { get() { return this.buffer }, configurable: true })
  Object.defineProperty(Buffer.prototype, "offset", { get() { return this.byteOffset }, configurable: true })
  const kMaxLength = 2 ** 32
  const kStringMaxLength = 2 ** 29 - 24
  function SlowBuffer(size) {
    return Buffer.alloc(size)
  }
  SlowBuffer.prototype = Buffer.prototype
  const bufferModule = {
    Buffer,
    SlowBuffer,
    kMaxLength,
    kStringMaxLength,
    INSPECT_MAX_BYTES: 50,
    constants: { MAX_LENGTH: kMaxLength, MAX_STRING_LENGTH: kStringMaxLength },
    atob: (s) => globalThis.atob(s),
    btoa: (s) => globalThis.btoa(s),
    isUtf8: (input) => {
      try {
        new TextDecoder("utf-8", { fatal: true }).decode(input)
        return true
      } catch {
        return false
      }
    },
    isAscii: (input) => {
      const bytes = input instanceof ArrayBuffer ? new Uint8Array(input) : input
      for (let i = 0; i < bytes.length; i++) if (bytes[i] > 127) return false
      return true
    },
    transcode: (source, fromEnc, toEnc) => Buffer.from(stringFromBytes(source, fromEnc), toEnc),
    resolveObjectURL: () => undefined,
    get Blob() {
      return globalThis.Blob
    },
    get File() {
      return globalThis.File
    },
  }
  for (const name of Object.getOwnPropertyNames(Buffer.prototype)) {
    if (/^(read|write)UInt/.test(name)) {
      Object.defineProperty(Buffer.prototype, name.replace("UInt", "Uint"), { value: Buffer.prototype[name], writable: true, configurable: true })
    }
  }

  const kRejection = Symbol.for("nodejs.rejection")
  function EventEmitter(opts) {
    EventEmitter.init.call(this, opts)
  }
  EventEmitter.defaultMaxListeners = 10
  EventEmitter.errorMonitor = Symbol("events.errorMonitor")
  EventEmitter.captureRejectionSymbol = kRejection
  EventEmitter.captureRejections = false
  EventEmitter.EventEmitter = EventEmitter
  EventEmitter.init = function init(opts) {
    if (this._events === undefined || this._events === Object.getPrototypeOf(this)?._events) {
      this._events = Object.create(null)
      this._eventsCount = 0
    }
    this._maxListeners = this._maxListeners || undefined
    if (opts?.captureRejections) this[kRejection] = true
  }
  const eventsOf = (self) => {
    if (self._events === undefined || self._events === null || self._events === Object.getPrototypeOf(self)?._events) {
      self._events = Object.create(null)
      self._eventsCount = 0
    }
    return self._events
  }
  const checkListener = (fn) => {
    if (typeof fn !== "function") throw new TypeError(`The "listener" argument must be of type function. Received ${fn === null ? "null" : typeof fn}`)
  }
  const attachListener = (self, name, fn, prepend) => {
    checkListener(fn)
    const events = eventsOf(self)
    if (events.newListener !== undefined) {
      self.emit("newListener", name, fn.listener ?? fn)
    }
    const existing = events[name]
    if (existing === undefined) {
      events[name] = fn
      self._eventsCount = (self._eventsCount ?? 0) + 1
    } else if (typeof existing === "function") {
      events[name] = prepend ? [fn, existing] : [existing, fn]
    } else if (prepend) existing.unshift(fn)
    else existing.push(fn)
    return self
  }
  const onceWrap = (self, name, fn) => {
    const state = { fired: false, wrapFn: undefined, target: self, type: name, listener: fn }
    const wrapper = function (...args) {
      if (state.fired) return
      state.target.removeListener(state.type, state.wrapFn)
      state.fired = true
      return state.listener.apply(state.target, args)
    }
    wrapper.listener = fn
    state.wrapFn = wrapper
    return wrapper
  }
  EventEmitter.prototype._events = undefined
  EventEmitter.prototype._eventsCount = 0
  EventEmitter.prototype._maxListeners = undefined
  EventEmitter.prototype.setMaxListeners = function setMaxListeners(n) {
    if (typeof n !== "number" || n < 0 || Number.isNaN(n)) throw new RangeError(`The value of "n" is out of range. It must be a non-negative number. Received ${n}`)
    this._maxListeners = n
    return this
  }
  EventEmitter.prototype.getMaxListeners = function getMaxListeners() {
    return this._maxListeners === undefined ? EventEmitter.defaultMaxListeners : this._maxListeners
  }
  EventEmitter.prototype.emit = function emit(name, ...args) {
    const events = eventsOf(this)
    if (name === "error" && events[EventEmitter.errorMonitor] !== undefined) this.emit(EventEmitter.errorMonitor, ...args)
    const handler = events[name]
    if (handler === undefined) {
      if (name === "error") {
        const err = args[0]
        if (err instanceof Error) throw err
        const e = new Error(`Unhandled error. (${typeof err === "string" ? `'${err}'` : String(err)})`)
        e.code = "ERR_UNHANDLED_ERROR"
        e.context = err
        throw e
      }
      return false
    }
    if (typeof handler === "function") {
      handler.apply(this, args)
    } else {
      for (const fn of handler.slice()) fn.apply(this, args)
    }
    return true
  }
  EventEmitter.prototype.addListener = function addListener(name, fn) {
    return attachListener(this, name, fn, false)
  }
  EventEmitter.prototype.on = EventEmitter.prototype.addListener
  EventEmitter.prototype.prependListener = function prependListener(name, fn) {
    return attachListener(this, name, fn, true)
  }
  EventEmitter.prototype.once = function once(name, fn) {
    checkListener(fn)
    return this.on(name, onceWrap(this, name, fn))
  }
  EventEmitter.prototype.prependOnceListener = function prependOnceListener(name, fn) {
    checkListener(fn)
    return this.prependListener(name, onceWrap(this, name, fn))
  }
  EventEmitter.prototype.removeListener = function removeListener(name, fn) {
    checkListener(fn)
    const events = eventsOf(this)
    const list = events[name]
    if (list === undefined) return this
    if (list === fn || list.listener === fn) {
      if (--this._eventsCount === 0) this._events = Object.create(null)
      else delete events[name]
      if (events.removeListener !== undefined) this.emit("removeListener", name, list.listener ?? fn)
    } else if (typeof list !== "function") {
      let position = -1
      for (let i = list.length - 1; i >= 0; i--) {
        if (list[i] === fn || list[i].listener === fn) {
          position = i
          break
        }
      }
      if (position < 0) return this
      const removed = list[position]
      if (position === 0) list.shift()
      else list.splice(position, 1)
      if (list.length === 1) events[name] = list[0]
      if (events.removeListener !== undefined) this.emit("removeListener", name, removed.listener ?? fn)
    }
    return this
  }
  EventEmitter.prototype.off = EventEmitter.prototype.removeListener
  EventEmitter.prototype.removeAllListeners = function removeAllListeners(name) {
    const events = eventsOf(this)
    if (events.removeListener === undefined) {
      if (name === undefined) {
        this._events = Object.create(null)
        this._eventsCount = 0
      } else if (events[name] !== undefined) {
        if (--this._eventsCount === 0) this._events = Object.create(null)
        else delete events[name]
      }
      return this
    }
    if (name === undefined) {
      for (const key of Reflect.ownKeys(events)) {
        if (key === "removeListener") continue
        this.removeAllListeners(key)
      }
      this.removeAllListeners("removeListener")
      this._events = Object.create(null)
      this._eventsCount = 0
      return this
    }
    const list = events[name]
    if (typeof list === "function") this.removeListener(name, list)
    else if (list !== undefined) for (let i = list.length - 1; i >= 0; i--) this.removeListener(name, list[i])
    return this
  }
  const listenersImpl = (self, name, unwrap) => {
    const events = self._events
    if (events === undefined || events === null) return []
    const list = events[name]
    if (list === undefined) return []
    if (typeof list === "function") return unwrap ? [list.listener ?? list] : [list]
    return unwrap ? list.map((l) => l.listener ?? l) : list.slice()
  }
  EventEmitter.prototype.listeners = function listeners(name) {
    return listenersImpl(this, name, true)
  }
  EventEmitter.prototype.rawListeners = function rawListeners(name) {
    return listenersImpl(this, name, false)
  }
  EventEmitter.prototype.listenerCount = function listenerCount(name, listener) {
    const events = this._events
    if (events === undefined || events === null) return 0
    const list = events[name]
    if (list === undefined) return 0
    if (typeof list === "function") return listener === undefined || list === listener || list.listener === listener ? 1 : 0
    if (listener === undefined) return list.length
    return list.filter((l) => l === listener || l.listener === listener).length
  }
  EventEmitter.prototype.eventNames = function eventNames() {
    return this._eventsCount > 0 && this._events ? Reflect.ownKeys(this._events) : []
  }
  EventEmitter.listenerCount = (emitter, name) => emitter.listenerCount(name)
  EventEmitter.getEventListeners = (target, name) => (typeof target.listeners === "function" ? target.listeners(name) : [])
  EventEmitter.getMaxListeners = (target) => (typeof target.getMaxListeners === "function" ? target.getMaxListeners() : EventEmitter.defaultMaxListeners)
  EventEmitter.setMaxListeners = (n, ...targets) => {
    if (targets.length === 0) EventEmitter.defaultMaxListeners = n
    else for (const t of targets) t.setMaxListeners?.(n)
  }
  EventEmitter.once = function once(emitter, name, options) {
    return new Promise((resolve, reject) => {
      const signal = options?.signal
      if (signal?.aborted) return reject(new DOMException("The operation was aborted", "AbortError"))
      const cleanup = () => {
        emitter.removeListener(name, onEvent)
        if (name !== "error" && typeof emitter.removeListener === "function") emitter.removeListener("error", onError)
        if (signal) signal.removeEventListener("abort", onAbort)
      }
      const onError = (err) => {
        cleanup()
        reject(err)
      }
      const onEvent = (...args) => {
        cleanup()
        resolve(args)
      }
      const onAbort = () => {
        cleanup()
        reject(signal.reason ?? new DOMException("The operation was aborted", "AbortError"))
      }
      if (typeof emitter.addEventListener === "function" && typeof emitter.once !== "function") {
        emitter.addEventListener(name, (ev) => onEvent(ev), { once: true })
      } else {
        emitter.once(name, onEvent)
        if (name !== "error") emitter.once("error", onError)
      }
      if (signal) signal.addEventListener("abort", onAbort, { once: true })
    })
  }
  EventEmitter.on = function on(emitter, name, options) {
    const queue = []
    let wake = null
    let error = null
    let done = false
    const push = (...args) => {
      queue.push(args)
      if (wake) wake()
    }
    emitter.on(name, push)
    if (name !== "error") emitter.on("error", (e) => {
      error = e
      if (wake) wake()
    })
    const signal = options?.signal
    if (signal) signal.addEventListener("abort", () => {
      error = signal.reason ?? new DOMException("The operation was aborted", "AbortError")
      if (wake) wake()
    }, { once: true })
    const iterator = {
      async next() {
        if (queue.length) return { value: queue.shift(), done: false }
        if (error) throw error
        if (done) return { value: undefined, done: true }
        await new Promise((r) => (wake = r))
        wake = null
        return iterator.next()
      },
      async return() {
        done = true
        emitter.removeListener(name, push)
        return { value: undefined, done: true }
      },
      [Symbol.asyncIterator]() {
        return this
      },
    }
    return iterator
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
  fs.realpath.native = fs.realpath
  fs.realpathSync.native = fs.realpathSync

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
    isAsyncFunction: (v) => typeof v === "function" && /^\[object Async(Generator)?Function\]$/.test(Object.prototype.toString.call(v)),
    isGeneratorFunction: (v) => typeof v === "function" && /GeneratorFunction\]$/.test(Object.prototype.toString.call(v)),
    isGeneratorObject: (v) => /^\[object (Async)?Generator\]$/.test(Object.prototype.toString.call(v)),
    isAnyArrayBuffer: (v) => v instanceof ArrayBuffer || (typeof SharedArrayBuffer === "function" && v instanceof SharedArrayBuffer),
    isSharedArrayBuffer: (v) => typeof SharedArrayBuffer === "function" && v instanceof SharedArrayBuffer,
    isArrayBufferView: (v) => ArrayBuffer.isView(v),
    isDataView: (v) => v instanceof DataView,
    isArgumentsObject: (v) => Object.prototype.toString.call(v) === "[object Arguments]",
    isBoxedPrimitive: (v) => v instanceof Number || v instanceof String || v instanceof Boolean || v instanceof Symbol || v instanceof BigInt,
    isNumberObject: (v) => v instanceof Number,
    isStringObject: (v) => v instanceof String,
    isBooleanObject: (v) => v instanceof Boolean,
    isSymbolObject: (v) => v instanceof Symbol,
    isBigIntObject: (v) => v instanceof BigInt,
    isMapIterator: (v) => Object.prototype.toString.call(v) === "[object Map Iterator]",
    isSetIterator: (v) => Object.prototype.toString.call(v) === "[object Set Iterator]",
    isWeakMap: (v) => v instanceof WeakMap,
    isWeakSet: (v) => v instanceof WeakSet,
    isModuleNamespaceObject: (v) => Object.prototype.toString.call(v) === "[object Module]",
    isProxy: () => false,
    isExternal: () => false,
    isKeyObject: () => false,
    isCryptoKey: (v) => typeof CryptoKey === "function" && v instanceof CryptoKey,
    isUint8ClampedArray: (v) => v instanceof Uint8ClampedArray,
    isUint16Array: (v) => v instanceof Uint16Array,
    isUint32Array: (v) => v instanceof Uint32Array,
    isInt8Array: (v) => v instanceof Int8Array,
    isInt16Array: (v) => v instanceof Int16Array,
    isInt32Array: (v) => v instanceof Int32Array,
    isFloat16Array: (v) => typeof Float16Array === "function" && v instanceof Float16Array,
    isFloat32Array: (v) => v instanceof Float32Array,
    isFloat64Array: (v) => v instanceof Float64Array,
    isBigInt64Array: (v) => v instanceof BigInt64Array,
    isBigUint64Array: (v) => v instanceof BigUint64Array,
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
  const kKeyBytes = Symbol("keyBytes")
  const unsupportedKeys = () => {
    const e = new Error("Asymmetric keys are not supported in this runtime")
    e.code = "ERR_FEATURE_UNAVAILABLE_ON_PLATFORM"
    return e
  }
  class KeyObject {
    #bytes
    constructor(type, bytes) {
      if (type !== "secret") throw unsupportedKeys()
      this.type = type
      this.#bytes = bytes
      this.symmetricKeySize = bytes.length
    }
    get [kKeyBytes]() {
      return this.#bytes
    }
    export(options) {
      if (options?.format === "jwk") return { kty: "oct", k: Buffer.from(this.#bytes).toString("base64url") }
      return Buffer.from(this.#bytes)
    }
    equals(other) {
      return other instanceof KeyObject && other.type === this.type && Buffer.from(this.#bytes).equals(other.export())
    }
    toCryptoKey() {
      throw unsupportedKeys()
    }
    static from() {
      throw unsupportedKeys()
    }
  }
  function cryptoBytes(data, encoding, what = "data") {
    if (data instanceof KeyObject) return data[kKeyBytes]
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
    KeyObject,
    createSecretKey: (key, encoding) => new KeyObject("secret", cryptoBytes(key, encoding, "key")),
    createPrivateKey: () => {
      throw unsupportedKeys()
    },
    createPublicKey: () => {
      throw unsupportedKeys()
    },
    generateKeySync: (type, options) => new KeyObject("secret", native.randomBytes(Math.ceil((options?.length ?? 256) / 8))),
    generateKey(type, options, cb) {
      let key
      let err = null
      try {
        key = crypto.generateKeySync(type, options)
      } catch (e) {
        err = e
      }
      queueMicrotask(() => cb(err, key))
    },
    generateKeyPairSync: () => {
      throw unsupportedKeys()
    },
    generateKeyPair(type, options, cb) {
      const done = typeof options === "function" ? options : cb
      queueMicrotask(() => done(unsupportedKeys()))
    },
    createSign: () => {
      throw unsupportedKeys()
    },
    createVerify: () => {
      throw unsupportedKeys()
    },
    sign: () => {
      throw unsupportedKeys()
    },
    verify: () => {
      throw unsupportedKeys()
    },
    createCipheriv: () => {
      const e = new Error("Ciphers are not supported in this runtime")
      e.code = "ERR_CRYPTO_UNKNOWN_CIPHER"
      throw e
    },
    createDecipheriv: () => {
      const e = new Error("Ciphers are not supported in this runtime")
      e.code = "ERR_CRYPTO_UNKNOWN_CIPHER"
      throw e
    },
    getCiphers: () => [],
    getCurves: () => [],
    pbkdf2Sync(password, salt, iterations, keylen, digest) {
      const out = native.pbkdf2(hashName(digest), cryptoBytes(password, undefined, "password"), cryptoBytes(salt, undefined, "salt"), iterations, keylen)
      return Buffer.from(out.buffer, out.byteOffset, out.byteLength)
    },
    pbkdf2(password, salt, iterations, keylen, digest, cb) {
      let out
      let err = null
      try {
        out = crypto.pbkdf2Sync(password, salt, iterations, keylen, digest)
      } catch (e) {
        err = e
      }
      queueMicrotask(() => cb(err, out))
    },
    hkdfSync(digest, ikm, salt, info, keylen) {
      const out = native.hkdf(hashName(digest), cryptoBytes(ikm, undefined, "ikm"), cryptoBytes(salt, undefined, "salt"), cryptoBytes(info, undefined, "info"), keylen)
      return out.buffer.slice(out.byteOffset, out.byteOffset + out.byteLength)
    },
    hkdf(digest, ikm, salt, info, keylen, cb) {
      let out
      let err = null
      try {
        out = crypto.hkdfSync(digest, ikm, salt, info, keylen)
      } catch (e) {
        err = e
      }
      queueMicrotask(() => cb(err, out))
    },
    scryptSync: () => {
      const e = new Error("scrypt is not supported in this runtime")
      e.code = "ERR_CRYPTO_SCRYPT_NOT_SUPPORTED"
      throw e
    },
    scrypt: (password, salt, keylen, options, cb) => {
      const done = typeof options === "function" ? options : cb
      const e = new Error("scrypt is not supported in this runtime")
      e.code = "ERR_CRYPTO_SCRYPT_NOT_SUPPORTED"
      queueMicrotask(() => done(e))
    },
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

  function Stream(options) {
    EventEmitter.call(this, options)
  }
  Object.setPrototypeOf(Stream.prototype, EventEmitter.prototype)
  Object.setPrototypeOf(Stream, EventEmitter)
  Stream.prototype.pipe = function pipe(dest, options) {
    const source = this
    source.on("data", (chunk) => {
      if (dest.writable === false || typeof dest.write !== "function") return
      if (dest.write(chunk) === false && typeof source.pause === "function") source.pause()
    })
    if (typeof dest.on === "function") {
      dest.on("drain", () => {
        if (typeof source.resume === "function") source.resume()
      })
    }
    if (!options || options.end !== false) {
      source.on("end", () => {
        if (typeof dest.end === "function") dest.end()
      })
    }
    if (typeof dest.emit === "function") dest.emit("pipe", source)
    return dest
  }

  function initReadable(stream, options) {
    stream[kRead] = {
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
    stream.readable = true
    stream.readableEnded = false
    stream.destroyed = false
    if (typeof options.read === "function") stream._read = options.read
    if (typeof options.destroy === "function") stream._destroy = options.destroy
    if (typeof options.construct === "function") queueMicrotask(() => options.construct.call(stream, (e) => e && stream.destroy(e)))
  }
  function scheduleRead(stream) {
    const r = stream[kRead]
    if (r.scheduled) return
    r.scheduled = true
    queueMicrotask(() => {
      r.scheduled = false
      flowRead(stream)
    })
  }
  function flowRead(stream) {
    const r = stream[kRead]
    if (r.destroyed) return
    if (r.flowing) {
      while (r.flowing && r.buffer.length && !r.destroyed) stream.emit("data", r.buffer.shift())
      if (r.flowing && !r.buffer.length && !r.ended && !r.destroyed) callRead(stream)
    } else if (r.buffer.length && stream.listenerCount("readable") > 0) {
      stream.emit("readable")
    }
    maybeEndRead(stream)
  }
  function callRead(stream) {
    const r = stream[kRead]
    if (r.reading || r.ended || r.destroyed) return
    r.reading = true
    try {
      stream._read(r.highWaterMark)
    } catch (e) {
      stream.destroy(e)
    }
    r.reading = false
  }
  function maybeEndRead(stream) {
    const r = stream[kRead]
    if (r.ended && r.buffer.length === 0 && !r.endEmitted && !r.destroyed) {
      r.endEmitted = true
      stream.readable = false
      stream.readableEnded = true
      queueMicrotask(() => {
        if (stream.listenerCount("readable") > 0) stream.emit("readable")
        stream.emit("end")
        if (r.autoDestroy && !stream[kWrite]) stream.destroy()
        else if (r.autoDestroy && stream[kWrite] && stream[kWrite].finished) stream.destroy()
      })
    }
  }

  function Readable(options) {
    options = options ?? {}
    if (this === undefined || this === globalThis) return new Readable(options)
    Stream.call(this, options)
    initReadable(this, options)
  }
  Object.setPrototypeOf(Readable.prototype, Stream.prototype)
  Object.setPrototypeOf(Readable, Stream)
  Object.defineProperties(Readable.prototype, {
    _readableState: { get() { return this[kRead] }, configurable: true },
    readableObjectMode: { get() { return this[kRead].objectMode }, configurable: true },
    readableHighWaterMark: { get() { return this[kRead].highWaterMark }, configurable: true },
    readableLength: { get() { return this[kRead].buffer.reduce((n, c) => n + (this[kRead].objectMode ? 1 : c.length), 0) }, configurable: true },
    readableFlowing: { get() { return this[kRead].flowing }, configurable: true },
    readableEncoding: { get() { return this[kRead].encoding }, configurable: true },
  })
  Object.assign(Readable.prototype, {
    _read() {},
    push(chunk, encoding) {
      const r = this[kRead]
      if (r.destroyed) return false
      if (chunk === null) {
        r.ended = true
        maybeEndRead(this)
        return false
      }
      let c = toChunk(this, chunk, encoding, r.objectMode)
      if (!r.objectMode && r.encoding && typeof c !== "string") c = c.toString(r.encoding)
      r.buffer.push(c)
      scheduleRead(this)
      return this.readableLength < r.highWaterMark
    },
    unshift(chunk) {
      this[kRead].buffer.unshift(chunk)
    },
    on(event, fn) {
      Stream.prototype.on.call(this, event, fn)
      if (event === "data" && this[kRead].flowing !== false) this.resume()
      if (event === "readable") {
        this[kRead].flowing = false
        scheduleRead(this)
        if (!this[kRead].buffer.length) queueMicrotask(() => callRead(this))
      }
      return this
    },
    resume() {
      const r = this[kRead]
      if (r.flowing !== true) {
        r.flowing = true
        scheduleRead(this)
      }
      return this
    },
    pause() {
      this[kRead].flowing = false
      return this
    },
    isPaused() {
      return this[kRead].flowing === false
    },
    read(size) {
      const r = this[kRead]
      if (!r.buffer.length && !r.ended) callRead(this)
      if (!r.buffer.length) {
        maybeEndRead(this)
        return null
      }
      if (r.objectMode || size === undefined) {
        const c = r.buffer.shift()
        maybeEndRead(this)
        if (!r.buffer.length && !r.ended) queueMicrotask(() => callRead(this))
        return c
      }
      let out = typeof r.buffer[0] === "string" ? r.buffer.join("") : Buffer.concat(r.buffer)
      r.buffer = []
      if (out.length > size) {
        r.buffer.push(out.slice(size))
        out = out.slice(0, size)
      }
      maybeEndRead(this)
      return out
    },
    setEncoding(encoding) {
      const r = this[kRead]
      r.encoding = encoding ?? "utf8"
      r.buffer = r.buffer.map((c) => (typeof c === "string" ? c : c.toString(r.encoding)))
      return this
    },
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
    },
    unpipe(dest) {
      const r = this[kRead]
      r.pipes = dest ? r.pipes.filter((d) => d !== dest) : []
      return this
    },
    destroy(err) {
      return destroyStream(this, err)
    },
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
    },
    async toArray() {
      const out = []
      for await (const c of this) out.push(c)
      return out
    },
  })
  Readable.from = function from(iterable, options = {}) {
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

  function Writable(options) {
    options = options ?? {}
    if (this === undefined || this === globalThis) return new Writable(options)
    Stream.call(this, options)
    initWritable(this, options)
    if (typeof options.construct === "function") queueMicrotask(() => options.construct.call(this, (e) => e && this.destroy(e)))
  }
  Object.setPrototypeOf(Writable.prototype, Stream.prototype)
  Object.setPrototypeOf(Writable, Stream)
  const writableGetters = {
    _writableState: { get() { return this[kWrite] }, configurable: true },
    writableObjectMode: { get() { return this[kWrite].objectMode }, configurable: true },
    writableHighWaterMark: { get() { return this[kWrite].highWaterMark }, configurable: true },
    writableLength: { get() { return this[kWrite].queue.length }, configurable: true },
    writableNeedDrain: { get() { return this[kWrite].needDrain }, configurable: true },
    writableCorked: { get() { return this[kWrite].corked }, configurable: true },
  }
  Object.defineProperties(Writable.prototype, writableGetters)
  Object.assign(Writable.prototype, writableMethods)

  function Duplex(options) {
    options = options ?? {}
    if (this === undefined || this === globalThis) return new Duplex(options)
    Readable.call(this, { ...options, read: options.read, objectMode: options.objectMode || options.readableObjectMode, highWaterMark: options.readableHighWaterMark ?? options.highWaterMark })
    initWritable(this, options)
    this.allowHalfOpen = options.allowHalfOpen !== false
  }
  Object.setPrototypeOf(Duplex.prototype, Readable.prototype)
  Object.setPrototypeOf(Duplex, Readable)
  Object.defineProperties(Duplex.prototype, writableGetters)
  for (const name of Object.keys(writableMethods)) if (name !== "destroy") Duplex.prototype[name] = writableMethods[name]

  function Transform(options) {
    options = options ?? {}
    if (this === undefined || this === globalThis) return new Transform(options)
    Duplex.call(this, options)
    if (typeof options.transform === "function") this._transform = options.transform
    if (typeof options.flush === "function") this._flush = options.flush
  }
  Object.setPrototypeOf(Transform.prototype, Duplex.prototype)
  Object.setPrototypeOf(Transform, Duplex)
  Object.assign(Transform.prototype, {
    _transform(chunk, encoding, cb) {
      cb(streamError("ERR_METHOD_NOT_IMPLEMENTED", "The _transform() method is not implemented"))
    },
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
    },
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
    },
  })

  function PassThrough(options) {
    if (this === undefined || this === globalThis) return new PassThrough(options)
    Transform.call(this, options)
  }
  Object.setPrototypeOf(PassThrough.prototype, Transform.prototype)
  Object.setPrototypeOf(PassThrough, Transform)
  PassThrough.prototype._transform = function _transform(chunk, encoding, cb) {
    cb(null, chunk)
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

  const stream = Object.assign(Stream, { Readable, Writable, Duplex, Transform, PassThrough, Stream, pipeline, finished, promises: streamPromises })
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

  const utilExtras = {
    inspectCustom: Symbol.for("nodejs.util.inspect.custom"),
    formatWithOptions: (options, ...args) => native.format(...args),
    stripVTControlCharacters: (s) => String(s).replace(/[\u001B\u009B][[\]()#;?]*(?:(?:(?:[a-zA-Z\d]*(?:;[-a-zA-Z\d\/#&.:=?%@~_]*)*)?\u0007)|(?:(?:\d{1,4}(?:;\d{0,4})*)?[\dA-PR-TZcf-nq-uy=><~]))/g, ""),
    debuglog(section) {
      const fn = () => {}
      fn.enabled = false
      return fn
    },
    getSystemErrorName: (code) => ({ "-2": "ENOENT", "-13": "EACCES", "-17": "EEXIST", "-22": "EINVAL", "-39": "ENOTEMPTY", "-98": "EADDRINUSE", "-111": "ECONNREFUSED", "-3008": "ENOTFOUND" })[String(code)] ?? `Unknown system error ${code}`,
    toUSVString: (s) => String(s).toWellFormed(),
    styleText: (format, text) => String(text),
    aborted: (signal, resource) => new Promise((resolve) => signal.aborted ? resolve() : signal.addEventListener("abort", () => resolve(), { once: true })),
    parseArgs(config = {}) {
      const args = config.args ?? process.argv.slice(2)
      const options = config.options ?? {}
      const strict = config.strict !== false
      const allowPositionals = config.allowPositionals ?? !strict
      const values = Object.create(null)
      const positionals = []
      const tokens = []
      const shortMap = {}
      for (const [name, o] of Object.entries(options)) if (o.short) shortMap[o.short] = name
      const define = (name, value, raw) => {
        const o = options[name]
        if (!o) {
          if (strict) throw new TypeError(`Unknown option '${raw}'`)
          values[name] = value ?? true
          return
        }
        if (o.type === "string" && value === undefined) throw new TypeError(`Option '${raw}' argument missing`)
        const v = o.type === "boolean" ? true : value
        if (o.multiple) (values[name] ??= []).push(v)
        else values[name] = v
      }
      let i = 0
      while (i < args.length) {
        const a = args[i]
        if (a === "--") {
          positionals.push(...args.slice(i + 1))
          break
        }
        if (a.startsWith("--")) {
          const eq = a.indexOf("=")
          const name = eq < 0 ? a.slice(2) : a.slice(2, eq)
          let value = eq < 0 ? undefined : a.slice(eq + 1)
          if (value === undefined && options[name]?.type === "string" && i + 1 < args.length) value = args[++i]
          define(name, value, a)
          tokens.push({ kind: "option", name, rawName: a.split("=")[0], value, index: i })
        } else if (a.startsWith("-") && a.length > 1) {
          const letters = a.slice(1)
          for (let j = 0; j < letters.length; j++) {
            const name = shortMap[letters[j]] ?? letters[j]
            let value
            if (options[name]?.type === "string") {
              value = j + 1 < letters.length ? letters.slice(j + 1) : args[++i]
              define(name, value, `-${letters[j]}`)
              break
            }
            define(name, undefined, `-${letters[j]}`)
          }
          tokens.push({ kind: "option", name: letters, rawName: a, index: i })
        } else {
          if (!allowPositionals) throw new TypeError(`Unexpected argument '${a}'`)
          positionals.push(a)
          tokens.push({ kind: "positional", value: a, index: i })
        }
        i++
      }
      for (const [name, o] of Object.entries(options)) if (!(name in values) && o.default !== undefined) values[name] = o.default
      const result = { values, positionals }
      if (config.tokens) result.tokens = tokens
      return result
    },
  }

  const isIPv4 = (s) => /^(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}$/.test(String(s))
  const isIPv6 = (s) => {
    s = String(s)
    if (!s.includes(":")) return false
    const zone = s.indexOf("%")
    if (zone >= 0) s = s.slice(0, zone)
    const parts = s.split("::")
    if (parts.length > 2) return false
    const check = (seg) => seg === "" || seg.split(":").every((h) => /^[0-9a-f]{1,4}$/i.test(h) || isIPv4(h))
    const count = s.split(":").filter((x) => x !== "").length
    return parts.every(check) && (parts.length === 2 ? count <= 7 : count === 8 || (s.includes(".") && count === 7))
  }
  const net = {
    isIPv4,
    isIPv6,
    isIP: (s) => (isIPv4(s) ? 4 : isIPv6(s) ? 6 : 0),
  }
  class Socket extends EventEmitter {
    constructor(options = {}) {
      super()
      this.connecting = false
      this.destroyed = false
      this.readable = false
      this.writable = false
      this.remoteAddress = undefined
      this.remotePort = undefined
    }
    connect(...args) {
      const cb = typeof args[args.length - 1] === "function" ? args.pop() : null
      this.connecting = true
      queueMicrotask(() => {
        this.connecting = false
        const e = new Error("net.Socket connections are not supported yet, use fetch or http.request")
        e.code = "ENOTSUP"
        this.emit("error", e)
        this.emit("close", true)
      })
      if (cb) this.once("connect", cb)
      return this
    }
    setEncoding() {
      return this
    }
    setKeepAlive() {
      return this
    }
    setNoDelay() {
      return this
    }
    setTimeout(ms, cb) {
      if (typeof cb === "function") this.once("timeout", cb)
      return this
    }
    write(data, encoding, cb) {
      if (typeof encoding === "function") cb = encoding
      if (typeof cb === "function") queueMicrotask(() => cb(new Error("socket is not connected")))
      return false
    }
    end() {
      return this
    }
    destroy(err) {
      if (this.destroyed) return this
      this.destroyed = true
      queueMicrotask(() => {
        if (err) this.emit("error", err)
        this.emit("close", !!err)
      })
      return this
    }
    ref() {
      return this
    }
    unref() {
      return this
    }
    address() {
      return {}
    }
  }
  net.Socket = Socket
  net.Stream = Socket
  net.createConnection = (...args) => new Socket().connect(...args)
  net.connect = net.createConnection
  class NetServer extends EventEmitter {
    constructor(options, listener) {
      super()
      if (typeof options === "function") listener = options
      if (typeof listener === "function") this.on("connection", listener)
      this.listening = false
    }
    listen() {
      throw new Error("net.createServer is not supported yet, use http.createServer or serve")
    }
    close(cb) {
      if (typeof cb === "function") queueMicrotask(cb)
      return this
    }
    address() {
      return null
    }
    ref() {
      return this
    }
    unref() {
      return this
    }
  }
  net.Server = NetServer
  net.createServer = (options, listener) => new NetServer(options, listener)
  net.BlockList = class BlockList {
    addAddress() {}
    addRange() {}
    addSubnet() {}
    check() {
      return false
    }
  }
  net.SocketAddress = class SocketAddress {
    constructor(options = {}) {
      this.address = options.address ?? "127.0.0.1"
      this.port = options.port ?? 0
      this.family = options.family ?? "ipv4"
      this.flowlabel = options.flowlabel ?? 0
    }
  }

  const tls = {
    connect() {
      throw new Error("tls.connect is not supported yet, use fetch or https.request")
    },
    createSecureContext: () => ({}),
    createServer() {
      throw new Error("tls.createServer is not supported yet")
    },
    checkServerIdentity: () => undefined,
    getCiphers: () => [],
    rootCertificates: [],
    DEFAULT_MIN_VERSION: "TLSv1.2",
    DEFAULT_MAX_VERSION: "TLSv1.3",
    DEFAULT_ECDH_CURVE: "auto",
  }
  tls.TLSSocket = class TLSSocket extends Socket {}
  tls.Server = class TLSServer extends NetServer {}

  const dnsLookupAll = (host) => native.dnsLookup(host).map(([address, family]) => ({ address, family }))
  const dnsPromises = {
    async lookup(host, options) {
      const opts = typeof options === "number" ? { family: options } : options ?? {}
      let list = dnsLookupAll(String(host))
      if (opts.family === 4 || opts.family === 6 || opts.family === "IPv4" || opts.family === "IPv6") {
        const want = opts.family === "IPv4" ? 4 : opts.family === "IPv6" ? 6 : opts.family
        const filtered = list.filter((e) => e.family === want)
        if (filtered.length) list = filtered
      }
      if (!list.length) {
        const e = new Error(`getaddrinfo ENOTFOUND ${host}`)
        e.code = "ENOTFOUND"
        throw e
      }
      return opts.all ? list : list[0]
    },
    async resolve4(host) {
      return dnsLookupAll(String(host)).filter((e) => e.family === 4).map((e) => e.address)
    },
    async resolve6(host) {
      return dnsLookupAll(String(host)).filter((e) => e.family === 6).map((e) => e.address)
    },
    async resolve(host) {
      return dnsLookupAll(String(host)).map((e) => e.address)
    },
    getServers: () => [],
    setServers() {},
  }
  const callbackify2 = (fn) => (...args) => {
    const cb = args.pop()
    fn(...args).then(
      (v) => (Array.isArray(v) || typeof v !== "object" ? cb(null, v) : cb(null, v.address, v.family)),
      (e) => cb(e),
    )
  }
  const dns = {
    lookup: callbackify2(dnsPromises.lookup),
    resolve4: callbackify2(dnsPromises.resolve4),
    resolve6: callbackify2(dnsPromises.resolve6),
    resolve: callbackify2(dnsPromises.resolve),
    getServers: () => [],
    setServers() {},
    promises: dnsPromises,
    ADDRCONFIG: 1024,
    V4MAPPED: 2048,
    ALL: 256,
  }

  const notSupported = (what) => () => {
    const e = new Error(`${what} is not supported yet`)
    e.code = "ENOTSUP"
    throw e
  }
  const http2 = {
    connect: notSupported("http2.connect"),
    createServer: notSupported("http2.createServer"),
    createSecureServer: notSupported("http2.createSecureServer"),
    getDefaultSettings: () => ({ headerTableSize: 4096, enablePush: true, initialWindowSize: 65535, maxFrameSize: 16384, maxConcurrentStreams: 4294967295, maxHeaderListSize: 65535 }),
    getPackedSettings: () => Buffer.alloc(0),
    sensitiveHeaders: Symbol("nodejs.http2.sensitiveHeaders"),
    constants: {
      HTTP2_HEADER_STATUS: ":status", HTTP2_HEADER_METHOD: ":method", HTTP2_HEADER_AUTHORITY: ":authority", HTTP2_HEADER_SCHEME: ":scheme", HTTP2_HEADER_PATH: ":path",
      HTTP2_HEADER_CONTENT_TYPE: "content-type", HTTP2_HEADER_CONTENT_LENGTH: "content-length", HTTP2_HEADER_ACCEPT: "accept", HTTP2_HEADER_USER_AGENT: "user-agent", HTTP2_HEADER_COOKIE: "cookie", HTTP2_HEADER_SET_COOKIE: "set-cookie",
      HTTP2_METHOD_GET: "GET", HTTP2_METHOD_POST: "POST", HTTP2_METHOD_PUT: "PUT", HTTP2_METHOD_DELETE: "DELETE", HTTP2_METHOD_HEAD: "HEAD", HTTP2_METHOD_PATCH: "PATCH", HTTP2_METHOD_OPTIONS: "OPTIONS",
      NGHTTP2_NO_ERROR: 0, NGHTTP2_PROTOCOL_ERROR: 1, NGHTTP2_INTERNAL_ERROR: 2, NGHTTP2_CANCEL: 8, NGHTTP2_REFUSED_STREAM: 7,
      HTTP_STATUS_OK: 200, HTTP_STATUS_CREATED: 201, HTTP_STATUS_NO_CONTENT: 204, HTTP_STATUS_BAD_REQUEST: 400, HTTP_STATUS_NOT_FOUND: 404, HTTP_STATUS_INTERNAL_SERVER_ERROR: 500,
    },
  }

  const workerThreads = {
    isMainThread: true,
    threadId: 0,
    parentPort: null,
    workerData: null,
    resourceLimits: {},
    SHARE_ENV: Symbol("nodejs.worker_threads.SHARE_ENV"),
    Worker: class Worker {
      constructor() {
        notSupported("worker_threads.Worker")()
      }
    },
    MessageChannel: class MessageChannel {
      constructor() {
        notSupported("MessageChannel")()
      }
    },
    MessagePort: class MessagePort extends EventEmitter {},
    BroadcastChannel: class BroadcastChannel {
      constructor() {
        notSupported("BroadcastChannel")()
      }
    },
    markAsUntransferable() {},
    moveMessagePortToContext: notSupported("moveMessagePortToContext"),
    receiveMessageOnPort: () => undefined,
    setEnvironmentData() {},
    getEnvironmentData: () => undefined,
  }

  const v8 = {
    getHeapStatistics() {
      const m = process.memoryUsage()
      return { total_heap_size: m.heapTotal, total_heap_size_executable: 0, total_physical_size: m.rss, total_available_size: 0, used_heap_size: m.heapUsed, heap_size_limit: 0, malloced_memory: 0, peak_malloced_memory: 0, does_zap_garbage: 0, number_of_native_contexts: 1, number_of_detached_contexts: 0, external_memory: m.external ?? 0 }
    },
    getHeapSpaceStatistics: () => [],
    getHeapCodeStatistics: () => ({}),
    setFlagsFromString() {},
    cachedDataVersionTag: () => 0,
    serialize: (v) => Buffer.from(JSON.stringify(v)),
    deserialize: (b) => JSON.parse(Buffer.from(b).toString()),
    writeHeapSnapshot: notSupported("v8.writeHeapSnapshot"),
    takeCoverage() {},
    stopCoverage() {},
  }

  const indirectEval = eval
  class VmScript {
    #code
    constructor(code, options) {
      this.#code = String(code)
      this.filename = typeof options === "string" ? options : options?.filename ?? "evalmachine.<anonymous>"
    }
    runInThisContext() {
      return indirectEval(this.#code)
    }
    runInContext(context) {
      return vmRun(this.#code, context)
    }
    runInNewContext(context = {}) {
      return vmRun(this.#code, context)
    }
    createCachedData() {
      return Buffer.alloc(0)
    }
  }
  const contexts = new WeakSet()
  const vmRun = (code, sandbox) => {
    if (sandbox === undefined || sandbox === null) return indirectEval(String(code))
    const fn = new Function("__sandbox__", `with (__sandbox__) { return eval(${JSON.stringify(String(code))}) }`)
    return fn(sandbox)
  }
  const vmModule = {
    Script: VmScript,
    createContext(sandbox = {}) {
      contexts.add(sandbox)
      return sandbox
    },
    isContext: (o) => contexts.has(o),
    runInThisContext: (code) => indirectEval(String(code)),
    runInContext: (code, context) => vmRun(code, context),
    runInNewContext: (code, context = {}) => vmRun(code, context),
    compileFunction: (code, params = []) => new Function(...params, String(code)),
    measureMemory: async () => ({ total: { jsMemoryEstimate: process.memoryUsage().heapUsed, jsMemoryRange: [0, 0] } }),
  }

  const cluster = Object.assign(new EventEmitter(), {
    isPrimary: true,
    isMaster: true,
    isWorker: false,
    workers: {},
    worker: undefined,
    settings: {},
    SCHED_NONE: 1,
    SCHED_RR: 2,
    schedulingPolicy: 2,
    fork: notSupported("cluster.fork"),
    setupPrimary() {},
    setupMaster() {},
    disconnect(cb) {
      if (typeof cb === "function") queueMicrotask(cb)
    },
  })

  const inspector = {
    open() {},
    close() {},
    url: () => undefined,
    waitForDebugger() {},
    console,
    Session: class Session extends EventEmitter {
      connect() {}
      connectToMainThread() {}
      disconnect() {}
      post(method, params, cb) {
        const callback = typeof params === "function" ? params : cb
        if (typeof callback === "function") queueMicrotask(() => callback(new Error("inspector is not supported")))
      }
    },
  }

  const channels = new Map()
  class Channel {
    #subs = new Set()
    constructor(name) {
      this.name = name
    }
    get hasSubscribers() {
      return this.#subs.size > 0
    }
    subscribe(fn) {
      this.#subs.add(fn)
    }
    unsubscribe(fn) {
      return this.#subs.delete(fn)
    }
    publish(message) {
      for (const fn of this.#subs) fn(message, this.name)
    }
    bindStore() {}
    unbindStore() {
      return false
    }
    runStores(data, fn, thisArg, ...args) {
      return fn.apply(thisArg, args)
    }
  }
  const diagnosticsChannel = {
    channel(name) {
      if (!channels.has(name)) channels.set(name, new Channel(name))
      return channels.get(name)
    },
    hasSubscribers: (name) => channels.get(name)?.hasSubscribers ?? false,
    subscribe(name, fn) {
      diagnosticsChannel.channel(name).subscribe(fn)
    },
    unsubscribe: (name, fn) => channels.get(name)?.unsubscribe(fn) ?? false,
    tracingChannel(name) {
      const prefix = typeof name === "string" ? name : ""
      const mk = (k) => diagnosticsChannel.channel(`tracing:${prefix}:${k}`)
      return { start: mk("start"), end: mk("end"), asyncStart: mk("asyncStart"), asyncEnd: mk("asyncEnd"), error: mk("error"), traceSync: (fn, ctx, thisArg, ...args) => fn.apply(thisArg, args), tracePromise: (fn, ctx, thisArg, ...args) => fn.apply(thisArg, args), traceCallback: (fn, pos, ctx, thisArg, ...args) => fn.apply(thisArg, args), subscribe() {}, unsubscribe() {} }
    },
    Channel,
  }

  class AsyncLocalStorage {
    #store
    #active = false
    static bind(fn) {
      return fn
    }
    static snapshot() {
      return (fn, ...args) => fn(...args)
    }
    getStore() {
      return this.#active ? this.#store : undefined
    }
    run(store, fn, ...args) {
      const prev = [this.#active, this.#store]
      this.#active = true
      this.#store = store
      try {
        return fn(...args)
      } finally {
        ;[this.#active, this.#store] = prev
      }
    }
    exit(fn, ...args) {
      const prev = [this.#active, this.#store]
      this.#active = false
      try {
        return fn(...args)
      } finally {
        ;[this.#active, this.#store] = prev
      }
    }
    enterWith(store) {
      this.#active = true
      this.#store = store
    }
    disable() {
      this.#active = false
      this.#store = undefined
    }
  }
  let asyncIdCounter = 1
  const asyncHooks = {
    AsyncLocalStorage,
    AsyncResource: class AsyncResource {
      constructor(type) {
        this.type = type
        this.id = asyncIdCounter++
      }
      runInAsyncScope(fn, thisArg, ...args) {
        return fn.apply(thisArg, args)
      }
      emitDestroy() {
        return this
      }
      asyncId() {
        return this.id
      }
      triggerAsyncId() {
        return 0
      }
      bind(fn) {
        return fn
      }
      static bind(fn) {
        return fn
      }
    },
    createHook: () => ({ enable() { return this }, disable() { return this } }),
    executionAsyncId: () => 1,
    triggerAsyncId: () => 0,
    executionAsyncResource: () => ({}),
  }

  const punycodeBase = 36
  const punycodeAdapt = (delta, numPoints, firstTime) => {
    delta = firstTime ? Math.floor(delta / 700) : delta >> 1
    delta += Math.floor(delta / numPoints)
    let k = 0
    while (delta > 455) {
      delta = Math.floor(delta / 35)
      k += punycodeBase
    }
    return k + Math.floor((36 * delta) / (delta + 38))
  }
  const punycodeDigit = (d) => String.fromCharCode(d + 22 + 75 * (d < 26))
  const punycodeEncode = (input) => {
    const cps = [...input].map((c) => c.codePointAt(0))
    let output = cps.filter((c) => c < 0x80).map((c) => String.fromCharCode(c)).join("")
    const basicLength = output.length
    let handled = basicLength
    if (basicLength) output += "-"
    let n = 128
    let delta = 0
    let bias = 72
    while (handled < cps.length) {
      const m = Math.min(...cps.filter((c) => c >= n))
      delta += (m - n) * (handled + 1)
      n = m
      for (const c of cps) {
        if (c < n) delta++
        if (c === n) {
          let q = delta
          for (let k = punycodeBase; ; k += punycodeBase) {
            const t = k <= bias ? 1 : k >= bias + 26 ? 26 : k - bias
            if (q < t) break
            output += punycodeDigit(t + ((q - t) % (punycodeBase - t)))
            q = Math.floor((q - t) / (punycodeBase - t))
          }
          output += punycodeDigit(q)
          bias = punycodeAdapt(delta, handled + 1, handled === basicLength)
          delta = 0
          handled++
        }
      }
      delta++
      n++
    }
    return output
  }
  const punycodeDecode = (input) => {
    const out = []
    const basic = Math.max(input.lastIndexOf("-"), 0)
    for (let i = 0; i < basic; i++) out.push(input.charCodeAt(i))
    let n = 128
    let bias = 72
    let i = 0
    for (let idx = basic > 0 ? basic + 1 : 0; idx < input.length; ) {
      const oldi = i
      let w = 1
      for (let k = punycodeBase; ; k += punycodeBase) {
        const code = input.charCodeAt(idx++)
        const digit = code - 48 < 10 ? code - 22 : code - 65 < 26 ? code - 65 : code - 97 < 26 ? code - 97 : punycodeBase
        i += digit * w
        const t = k <= bias ? 1 : k >= bias + 26 ? 26 : k - bias
        if (digit < t) break
        w *= punycodeBase - t
      }
      bias = punycodeAdapt(i - oldi, out.length + 1, oldi === 0)
      n += Math.floor(i / (out.length + 1))
      i %= out.length + 1
      out.splice(i++, 0, n)
    }
    return String.fromCodePoint(...out)
  }
  const punycode = {
    version: "2.3.1",
    encode: punycodeEncode,
    decode: punycodeDecode,
    toASCII: (domain) => String(domain).split(".").map((label) => (/[^\x00-\x7f]/.test(label) ? "xn--" + punycodeEncode(label.toLowerCase()) : label)).join("."),
    toUnicode: (domain) => String(domain).split(".").map((label) => (label.toLowerCase().startsWith("xn--") ? punycodeDecode(label.slice(4)) : label)).join("."),
    ucs2: { decode: (s) => [...String(s)].map((c) => c.codePointAt(0)), encode: (cps) => String.fromCodePoint(...cps) },
  }

  const consoleMethods = ["log", "info", "debug", "warn", "error", "dir", "dirxml", "table", "group", "groupCollapsed", "groupEnd", "assert", "time", "timeEnd", "timeLog", "count", "countReset", "trace", "clear"]
  class Console {
    #out
    #err
    #indent = ""
    #timers = new Map()
    #counts = new Map()
    #ignoreErrors
    constructor(stdout, stderr, ignoreErrors) {
      const options = stdout && typeof stdout.write !== "function" ? stdout : { stdout, stderr, ignoreErrors }
      const out = options.stdout
      if (!out || typeof out.write !== "function") {
        const e = new TypeError("Console expects a writable stream instance for stdout")
        e.code = "ERR_CONSOLE_WRITABLE_STREAM"
        throw e
      }
      this.#out = out
      this.#err = options.stderr && typeof options.stderr.write === "function" ? options.stderr : out
      this.#ignoreErrors = options.ignoreErrors !== false
      this.#indent = " ".repeat(options.groupIndentation ?? 0).slice(0, 0)
      Object.defineProperty(this, "_stdout", { value: out, writable: true, enumerable: false, configurable: true })
      Object.defineProperty(this, "_stderr", { value: this.#err, writable: true, enumerable: false, configurable: true })
      for (const key of consoleMethods) {
        Object.defineProperty(this, key, { value: Console.prototype[key].bind(this), writable: true, enumerable: true, configurable: true })
      }
    }
    #write(stream, args) {
      const text = native.format(...args)
      const line = this.#indent ? text.split("\n").map((l) => this.#indent + l).join("\n") : text
      try {
        stream.write(line + "\n")
      } catch (e) {
        if (!this.#ignoreErrors) throw e
      }
    }
    log(...args) {
      this.#write(this.#out, args)
    }
    info(...args) {
      this.#write(this.#out, args)
    }
    debug(...args) {
      this.#write(this.#out, args)
    }
    dirxml(...args) {
      this.#write(this.#out, args)
    }
    warn(...args) {
      this.#write(this.#err, args)
    }
    error(...args) {
      this.#write(this.#err, args)
    }
    dir(value) {
      this.#write(this.#out, [value])
    }
    table(data, columns) {
      const text = globalThis["%renderTable%"](data, columns)
      this.#write(this.#out, [text === null ? data : text])
    }
    group(...args) {
      if (args.length) this.log(...args)
      this.#indent += "  "
    }
    groupCollapsed(...args) {
      this.group(...args)
    }
    groupEnd() {
      this.#indent = this.#indent.slice(0, -2)
    }
    assert(condition, ...args) {
      if (condition) return
      if (args.length && typeof args[0] === "string") args[0] = `Assertion failed: ${args[0]}`
      else args.unshift("Assertion failed")
      this.warn(...args)
    }
    time(label = "default") {
      label = String(label)
      if (this.#timers.has(label)) this.warn(`Warning: Label '${label}' already exists for console.time()`)
      else this.#timers.set(label, performance.now())
    }
    timeLog(label = "default", ...rest) {
      label = String(label)
      if (!this.#timers.has(label)) this.warn(`Warning: No such label '${label}' for console.timeLog()`)
      else this.log(`${label}: ${(performance.now() - this.#timers.get(label)).toFixed(3)}ms`, ...rest)
    }
    timeEnd(label = "default") {
      label = String(label)
      if (!this.#timers.has(label)) this.warn(`Warning: No such label '${label}' for console.timeEnd()`)
      else {
        this.log(`${label}: ${(performance.now() - this.#timers.get(label)).toFixed(3)}ms`)
        this.#timers.delete(label)
      }
    }
    count(label = "default") {
      label = String(label)
      const n = (this.#counts.get(label) ?? 0) + 1
      this.#counts.set(label, n)
      this.log(`${label}: ${n}`)
    }
    countReset(label = "default") {
      label = String(label)
      if (!this.#counts.has(label)) this.warn(`Warning: Count for '${label}' does not exist`)
      else this.#counts.set(label, 0)
    }
    trace(...args) {
      const stack = (new Error().stack ?? "").split("\n").slice(1).filter((l) => !l.includes("<squirt:")).join("\n")
      this.error(args.length ? "Trace:" : "Trace", ...args)
      if (stack) this.error(stack)
    }
    clear() {}
  }

  const consumeChunks = async (source) => {
    const chunks = []
    for await (const chunk of source) {
      if (typeof chunk === "string") chunks.push(Buffer.from(chunk))
      else if (chunk instanceof ArrayBuffer) chunks.push(Buffer.from(chunk))
      else if (ArrayBuffer.isView(chunk)) chunks.push(Buffer.from(chunk.buffer, chunk.byteOffset, chunk.byteLength))
      else {
        const e = new TypeError("Received a chunk that is not a string, Buffer, TypedArray or ArrayBuffer")
        e.code = "ERR_INVALID_ARG_TYPE"
        throw e
      }
    }
    return Buffer.concat(chunks)
  }
  const streamConsumers = {
    buffer: consumeChunks,
    arrayBuffer: async (source) => {
      const b = await consumeChunks(source)
      return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength)
    },
    text: async (source) => (await consumeChunks(source)).toString("utf8"),
    json: async (source) => JSON.parse((await consumeChunks(source)).toString("utf8")),
    blob: async (source) => new Blob([await consumeChunks(source)]),
  }

  const abortReason = (signal) => signal.reason ?? new DOMException("The operation was aborted", "AbortError")
  const timersPromises = {
    setTimeout: (ms = 1, value, options) =>
      new Promise((resolve, reject) => {
        const signal = options?.signal
        if (signal?.aborted) return reject(abortReason(signal))
        const t = setTimeout(() => resolve(value), ms)
        if (options?.ref === false) t.unref?.()
        signal?.addEventListener(
          "abort",
          () => {
            clearTimeout(t)
            reject(abortReason(signal))
          },
          { once: true },
        )
      }),
    setImmediate: (value, options) =>
      new Promise((resolve, reject) => {
        const signal = options?.signal
        if (signal?.aborted) return reject(abortReason(signal))
        const t = setImmediate(() => resolve(value))
        signal?.addEventListener(
          "abort",
          () => {
            clearImmediate(t)
            reject(abortReason(signal))
          },
          { once: true },
        )
      }),
    setInterval: async function* (ms = 1, value, options) {
      const signal = options?.signal
      while (true) {
        if (signal?.aborted) throw abortReason(signal)
        await timersPromises.setTimeout(ms, undefined, options)
        yield value
      }
    },
  }
  timersPromises.scheduler = {
    wait: (ms, options) => timersPromises.setTimeout(ms, undefined, options),
    yield: () => timersPromises.setImmediate(),
  }

  class Domain extends EventEmitter {
    constructor() {
      super()
      this.members = []
    }
    run(fn, ...args) {
      return fn.apply(this, args)
    }
    add(emitter) {
      if (!this.members.includes(emitter)) this.members.push(emitter)
    }
    remove(emitter) {
      const i = this.members.indexOf(emitter)
      if (i >= 0) this.members.splice(i, 1)
    }
    bind(fn) {
      return fn
    }
    intercept(fn) {
      return (err, ...args) => {
        if (err) this.emit("error", err)
        else fn(...args)
      }
    }
    enter() {}
    exit() {}
    dispose() {
      this.members.length = 0
    }
  }
  const domain = { Domain, create: () => new Domain(), createDomain: () => new Domain(), active: null }

  const traceEvents = {
    createTracing: (options) => {
      if (!options || !Array.isArray(options.categories)) throw new TypeError('The "options.categories" property must be of type Array')
      const tracing = {
        categories: options.categories.join(","),
        enabled: false,
        enable() {
          this.enabled = true
        },
        disable() {
          this.enabled = false
        },
      }
      return tracing
    },
    getEnabledCategories: () => undefined,
  }

  class Recoverable extends SyntaxError {
    constructor(err) {
      super(err?.message)
      this.err = err
    }
  }
  class REPLServer extends EventEmitter {
    constructor(options = {}) {
      super()
      this.commands = Object.create(null)
      this.context = globalThis
      this.input = options.input ?? process.stdin
      this.output = options.output ?? process.stdout
      this.useGlobal = true
      this._prompt = options.prompt ?? "> "
    }
    defineCommand(keyword, cmd) {
      this.commands[keyword] = typeof cmd === "function" ? { action: cmd } : cmd
    }
    displayPrompt() {
      this.output.write(this._prompt)
    }
    setPrompt(prompt) {
      this._prompt = prompt
    }
    getPrompt() {
      return this._prompt
    }
    clearBufferedCommand() {}
    setupHistory(path, cb) {
      cb?.(null, this)
    }
    close() {
      this.emit("exit")
    }
  }
  const repl = {
    start: (options) => new REPLServer(typeof options === "string" ? { prompt: options } : options),
    REPLServer,
    Recoverable,
    REPL_MODE_SLOPPY: Symbol("repl-sloppy"),
    REPL_MODE_STRICT: Symbol("repl-strict"),
    builtinModules: [],
  }

  const seaError = () => {
    const e = new Error("Single executable application is not available")
    e.code = "ERR_NOT_IN_SINGLE_EXECUTABLE_APPLICATION"
    return e
  }
  const sea = {
    isSea: () => false,
    getAsset: () => {
      throw seaError()
    },
    getAssetAsBlob: () => {
      throw seaError()
    },
    getRawAsset: () => {
      throw seaError()
    },
  }

  class WASI {
    constructor(options = {}) {
      this.wasiImport = {}
      this.version = options.version ?? "preview1"
    }
    getImportObject() {
      return { wasi_snapshot_preview1: this.wasiImport }
    }
    start() {
      throw new Error("WASI is not supported in this runtime")
    }
    initialize() {}
  }
  const wasi = { WASI }

  const makeNodeTest = (base) => {
    const normalize = (name, options, fn) => {
      if (typeof name === "function") return [name.name || "<anonymous>", name]
      if (typeof options === "function") return [String(name), options]
      return [String(name), fn ?? (() => {})]
    }
    const test = (name, options, fn) => {
      const [title, body] = normalize(name, options, fn)
      return base.test(title, body)
    }
    const describe = (name, options, fn) => {
      const [title, body] = normalize(name, options, fn)
      return base.describe(title, body)
    }
    const skip = () => undefined
    const mockFn = (original, implementation, options) => {
      if (typeof original !== "function") original = () => {}
      let impl = implementation ?? original
      const calls = []
      const queue = []
      const f = function (...args) {
        const next = queue.shift()
        const target = next ?? impl
        let result
        let error
        try {
          result = new.target ? new target(...args) : target.apply(this, args)
        } catch (e) {
          error = e
          throw e
        } finally {
          calls.push({ arguments: args, result, error, this: this, target: new.target, stack: new Error().stack })
        }
        return result
      }
      f.mock = {
        calls,
        callCount: () => calls.length,
        mockImplementation(fn) {
          impl = fn
        },
        mockImplementationOnce(fn) {
          queue.push(fn)
        },
        resetCalls() {
          calls.length = 0
        },
        restore() {
          impl = original
        },
      }
      if (options?.times !== undefined) {
        let left = options.times
        const chosen = impl
        impl = function (...args) {
          if (left-- > 0) return chosen.apply(this, args)
          return original.apply(this, args)
        }
      }
      return f
    }
    const mock = {
      fn: mockFn,
      method(object, name, implementation, options) {
        const original = object[name]
        const f = mockFn(original, implementation, options)
        const restore = f.mock.restore
        f.mock.restore = () => {
          restore()
          object[name] = original
        }
        object[name] = f
        return f
      },
      getter(object, name, implementation) {
        const f = mockFn(() => Reflect.get(object, name), implementation)
        Object.defineProperty(object, name, { get: f, configurable: true })
        return f
      },
      setter(object, name, implementation) {
        const f = mockFn((v) => Reflect.set(object, name, v), implementation)
        Object.defineProperty(object, name, { set: f, configurable: true })
        return f
      },
      module() {
        throw new Error("mock.module is not supported")
      },
      reset() {},
      restoreAll() {},
      timers: { enable() {}, reset() {}, tick() {}, runAll() {}, setTime() {} },
    }
    const before = (fn) => {
      let done = false
      base.beforeEach(async () => {
        if (done) return
        done = true
        await fn()
      })
    }
    const after = (fn) => {
      process.on("exit", () => fn())
    }
    Object.assign(test, {
      test,
      it: test,
      describe,
      suite: describe,
      before,
      after,
      beforeEach: base.beforeEach,
      afterEach: base.afterEach,
      skip,
      todo: skip,
      only: test,
      mock,
      run: () => new EventEmitter(),
      snapshot: { setDefaultSnapshotSerializers() {}, setResolveSnapshotPath() {} },
    })
    test.describe.skip = skip
    test.describe.todo = skip
    test.describe.only = describe
    test.it.skip = skip
    test.it.todo = skip
    test.it.only = test
    Object.defineProperty(test, "default", { value: test, writable: true, enumerable: false, configurable: true })
    return test
  }

  function Url() {
    this.protocol = null
    this.slashes = null
    this.auth = null
    this.host = null
    this.port = null
    this.hostname = null
    this.hash = null
    this.search = null
    this.query = null
    this.pathname = null
    this.path = null
    this.href = null
  }
  const slashedProtocols = /^(https?|ftp|gopher|file|ws|wss):$/
  function urlFormat(obj, options) {
    if (typeof obj === "string") obj = urlParse(obj)
    if (obj instanceof URL) {
      let href = obj.href
      if (options?.fragment === false) href = href.replace(/#.*$/, "")
      if (options?.search === false) href = href.replace(/\?[^#]*/, "")
      if (options?.auth === false && (obj.username || obj.password)) href = href.replace(`${obj.username}${obj.password ? ":" + obj.password : ""}@`, "")
      return href
    }
    if (obj === null || typeof obj !== "object") throw new TypeError('The "urlObject" argument must be one of type object or string')
    const auth = obj.auth ? encodeURIComponent(obj.auth).replace(/%3A/i, ":") + "@" : ""
    let protocol = obj.protocol ?? ""
    if (protocol && !protocol.endsWith(":")) protocol += ":"
    let host = ""
    if (obj.host) host = auth + obj.host
    else if (obj.hostname) host = auth + (obj.hostname.includes(":") ? `[${obj.hostname}]` : obj.hostname) + (obj.port ? `:${obj.port}` : "")
    let pathname = obj.pathname ?? ""
    let search = obj.search ?? ""
    if (!search && obj.query) search = typeof obj.query === "object" ? (Object.keys(obj.query).length ? "?" + querystring.stringify(obj.query) : "") : `?${obj.query}`
    let hash = obj.hash ?? ""
    if (search && !search.startsWith("?")) search = "?" + search
    if (hash && !hash.startsWith("#")) hash = "#" + hash
    const slashes = obj.slashes || (protocol && slashedProtocols.test(protocol))
    if (slashes || host) {
      host = "//" + host
      if (pathname && !pathname.startsWith("/")) pathname = "/" + pathname
    }
    pathname = pathname.replace(/[?#]/g, encodeURIComponent)
    search = search.replace("#", "%23")
    return protocol + host + pathname + search + hash
  }
  function urlParse(input, parseQueryString = false, slashesDenoteHost = false) {
    if (input instanceof Url) return input
    if (typeof input !== "string") throw new TypeError('The "url" argument must be of type string')
    const u = new Url()
    let rest = input.trim()
    const hashIdx = rest.indexOf("#")
    if (hashIdx >= 0) {
      u.hash = rest.slice(hashIdx)
      rest = rest.slice(0, hashIdx)
    }
    const qIdx = rest.indexOf("?")
    if (qIdx >= 0) {
      u.search = rest.slice(qIdx)
      rest = rest.slice(0, qIdx)
    }
    const m = /^([a-zA-Z][a-zA-Z0-9+.-]*:)/.exec(rest)
    if (m) {
      u.protocol = m[1].toLowerCase()
      rest = rest.slice(m[1].length)
    }
    if (rest.startsWith("//") && (u.protocol || slashesDenoteHost)) {
      u.slashes = true
      rest = rest.slice(2)
      const end = rest.search(/[/\\?#]/)
      let authority = end >= 0 ? rest.slice(0, end) : rest
      rest = end >= 0 ? rest.slice(end) : ""
      const at = authority.lastIndexOf("@")
      if (at >= 0) {
        u.auth = decodeURIComponent(authority.slice(0, at))
        authority = authority.slice(at + 1)
      }
      const portMatch = /:(\d*)$/.exec(authority)
      if (portMatch && !authority.endsWith("]")) {
        u.port = portMatch[1] || null
        authority = authority.slice(0, authority.length - portMatch[0].length)
      }
      const lower = authority.toLowerCase()
      u.hostname = lower.startsWith("[") && lower.endsWith("]") ? lower.slice(1, -1) : lower
      u.host = u.port ? `${lower}:${u.port}` : lower
      if (!rest && u.protocol) rest = "/"
    }
    u.pathname = rest || null
    if (parseQueryString) u.query = querystring.parse(u.search ? u.search.slice(1) : "")
    else u.query = u.search !== null ? u.search.slice(1) : null
    if (u.search === "" || u.search === "?") u.search = u.search === null ? null : u.search
    u.path = u.pathname === null && u.search === null ? null : (u.pathname ?? "") + (u.search ?? "")
    u.href = urlFormat(u)
    return u
  }
  const urlLegacy = {
    Url,
    parse: urlParse,
    format: urlFormat,
    resolve(from, to) {
      try {
        return new URL(to, from).href
      } catch {
        return to
      }
    },
    resolveObject(from, to) {
      return urlParse(urlLegacy.resolve(typeof from === "string" ? from : urlFormat(from), to))
    },
    domainToASCII: (domain) => punycode.toASCII(String(domain).toLowerCase()),
    domainToUnicode: (domain) => punycode.toUnicode(String(domain)),
    urlToHttpOptions(url) {
      const hostname = url.hostname.startsWith("[") ? url.hostname.slice(1, -1) : url.hostname
      const options = { protocol: url.protocol, hostname, hash: url.hash, search: url.search, pathname: url.pathname, path: `${url.pathname || ""}${url.search || ""}`, href: url.href }
      if (url.port !== "") options.port = Number(url.port)
      if (url.username || url.password) options.auth = `${decodeURIComponent(url.username)}:${decodeURIComponent(url.password)}`
      return options
    },
  }

  const nodeConstants = { O_RDONLY: 0, O_WRONLY: 1, O_RDWR: 2, S_IFMT: 61440, S_IFREG: 32768, S_IFDIR: 16384, S_IFLNK: 40960, F_OK: 0, R_OK: 4, W_OK: 2, X_OK: 1, SIGINT: 2, SIGTERM: 15, SIGKILL: 9, SIGHUP: 1, E2BIG: 7, EACCES: 13, EEXIST: 17, ENOENT: 2, ENOTDIR: 20, EISDIR: 21, EINVAL: 22, ENOTEMPTY: 39, UV_UDP_REUSEADDR: 4 }

  return {
    crypto,
    zlib,
    stream,
    utilExtras,
    http2,
    workerThreads,
    v8,
    vmModule,
    cluster,
    inspector,
    diagnosticsChannel,
    asyncHooks,
    punycode,
    nodeConstants,
    net,
    tls,
    dns,
    dnsPromises,
    streamPromises,
    httpClient,
    StringDecoder,
    querystring,
    tty,
    PerformanceObserver,
    Buffer,
    bufferModule,
    Console,
    streamConsumers,
    timersPromises,
    domain,
    traceEvents,
    repl,
    sea,
    wasi,
    makeNodeTest,
    urlLegacy,
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
    let natives: [(&str, u32, crate::object::NativeFn); 31] = [
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
        ("pbkdf2", 5, crypto_pbkdf2),
        ("hkdf", 5, crypto_hkdf),
        ("zlib", 3, zlib_op),
        ("crc32", 2, zlib_crc32),
        ("format", 0, native_format),
        ("dnsLookup", 1, dns_lookup),
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
    o.set_hidden("default", Value::Object(o.clone()));
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
        copy.set_hidden("default", v.clone());
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
                    h.set_hidden("default", Value::Object(h.clone()));
                }
                h
            }
            "stream" => {
                let n = load(vm)?;
                let s = vm.get_prop(&n, "stream")?;
                if let Value::Object(o) = &s {
                    o.set_hidden("default", s.clone());
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
            "net" | "tls" | "http2" | "v8" | "cluster" | "inspector" | "punycode" | "domain"
            | "repl" | "sea" | "wasi" => {
                let n = load(vm)?;
                let m = vm.get_prop(&n, name)?;
                with_default(vm, m)?
            }
            "trace_events" => {
                let n = load(vm)?;
                let m = vm.get_prop(&n, "traceEvents")?;
                with_default(vm, m)?
            }
            "stream/consumers" => {
                let n = load(vm)?;
                let m = vm.get_prop(&n, "streamConsumers")?;
                with_default(vm, m)?
            }
            "stream/web" => {
                let o = vm.new_object();
                for key in [
                    "ReadableStream",
                    "ReadableStreamDefaultReader",
                    "ReadableStreamBYOBReader",
                    "ReadableStreamDefaultController",
                    "ReadableByteStreamController",
                    "ReadableStreamBYOBRequest",
                    "WritableStream",
                    "WritableStreamDefaultWriter",
                    "WritableStreamDefaultController",
                    "TransformStream",
                    "TransformStreamDefaultController",
                    "ByteLengthQueuingStrategy",
                    "CountQueuingStrategy",
                    "TextEncoderStream",
                    "TextDecoderStream",
                    "CompressionStream",
                    "DecompressionStream",
                ] {
                    vm.load_lazy_global(key)?;
                    if let Some(v) = vm.globals.get(key).cloned() {
                        o.set_own(key, v);
                    }
                }
                o.set_hidden("default", Value::Object(o.clone()));
                Value::Object(o)
            }
            "util/types" => {
                let n = load(vm)?;
                let t = vm.get_prop(&n, "types")?;
                with_default(vm, t)?
            }
            "console" => {
                let n = load(vm)?;
                let console = vm.globals.get("console").cloned().unwrap_or_default();
                let class = vm.get_prop(&n, "Console")?;
                if let Value::Object(c) = &console {
                    c.set_hidden("Console", class);
                    c.set_hidden("default", console.clone());
                }
                console
            }
            "test" => {
                let n = load(vm)?;
                let make = vm.get_prop(&n, "makeNodeTest")?;
                let base = vm.import_builtin("test")?;
                vm.call_value(&make, Value::Undefined, &[base])?
            }
            "worker_threads" => {
                let n = load(vm)?;
                let m = vm.get_prop(&n, "workerThreads")?;
                with_default(vm, m)?
            }
            "vm" => {
                let n = load(vm)?;
                let m = vm.get_prop(&n, "vmModule")?;
                with_default(vm, m)?
            }
            "diagnostics_channel" => {
                let n = load(vm)?;
                let m = vm.get_prop(&n, "diagnosticsChannel")?;
                with_default(vm, m)?
            }
            "async_hooks" => {
                let n = load(vm)?;
                let m = vm.get_prop(&n, "asyncHooks")?;
                with_default(vm, m)?
            }
            "constants" => {
                let n = load(vm)?;
                let m = vm.get_prop(&n, "nodeConstants")?;
                with_default(vm, m)?
            }
            "inspector/promises" => {
                let n = load(vm)?;
                let m = vm.get_prop(&n, "inspector")?;
                with_default(vm, m)?
            }
            "dns" => {
                let n = load(vm)?;
                let d = vm.get_prop(&n, "dns")?;
                with_default(vm, d)?
            }
            "dns/promises" => {
                let n = load(vm)?;
                let d = vm.get_prop(&n, "dnsPromises")?;
                with_default(vm, d)?
            }
            "module" => {
                let o = vm.new_object();
                let require = vm.globals.get("require").cloned().unwrap_or_default();
                let create = native_fn(vm, "createRequire", 1, module_create_require);
                o.set_own("createRequire", create);
                let is_builtin = native_fn(vm, "isBuiltin", 1, module_is_builtin);
                o.set_own("isBuiltin", is_builtin);
                let names: Vec<Value> = BUILTIN_MODULES.iter().map(|n| Value::from(*n)).collect();
                let list = vm.new_array(names);
                o.set_own("builtinModules", list);
                o.set_own("require", require);
                o.set_hidden("default", Value::Object(o.clone()));
                Value::Object(o)
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
                let b = vm.get_prop(&n, "bufferModule")?;
                with_default(vm, b)?
            }
            "assert" | "assert/strict" => {
                let n = load(vm)?;
                let a = vm.get_prop(&n, "assert")?;
                if let Value::Object(f) = &a {
                    f.set_hidden("default", a.clone());
                }
                a
            }
            "sys" => vm.import_node("util")?.unwrap_or_default(),
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
                let extras = vm.get_prop(&n, "utilExtras")?;
                let custom = vm.get_prop(&extras, "inspectCustom")?;
                for (k, x) in vm.own_entries(&extras) {
                    if &*k != "inspectCustom" {
                        o.set_own(k, x);
                    }
                }
                if let Some(Value::Object(inspect)) = o.get_own("inspect") {
                    if inspect.get_own("custom").is_none() {
                        inspect.set_own("custom", custom);
                        let defaults = vm.new_object();
                        inspect.set_own("defaultOptions", Value::Object(defaults));
                        let colors = vm.new_object();
                        inspect.set_own("colors", Value::Object(colors));
                        let styles = vm.new_object();
                        inspect.set_own("styles", Value::Object(styles));
                    }
                }
                let te = vm.globals.get("TextEncoder").cloned().unwrap_or_default();
                let td = vm.globals.get("TextDecoder").cloned().unwrap_or_default();
                o.set_own("TextEncoder", te);
                o.set_own("TextDecoder", td);
                o.set_hidden("default", Value::Object(o.clone()));
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
                let legacy = vm.get_prop(&n, "urlLegacy")?;
                for (k, v) in vm.own_entries(&legacy) {
                    o.set_own(k, v);
                }
                o.set_hidden("default", Value::Object(o.clone()));
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
                o.set_hidden("default", Value::Object(o.clone()));
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
                let n = load(vm)?;
                let o = vm.new_object();
                for key in [
                    "setTimeout",
                    "setInterval",
                    "setImmediate",
                    "clearTimeout",
                    "clearInterval",
                    "clearImmediate",
                ] {
                    let v = vm.globals.get(key).cloned().unwrap_or_default();
                    o.set_own(key, v);
                }
                let p = vm.get_prop(&n, "timersPromises")?;
                o.set_own("promises", p);
                o.set_hidden("default", Value::Object(o.clone()));
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
                o.set_hidden("default", Value::Object(o.clone()));
                Value::Object(o)
            }
            "timers/promises" => {
                let n = load(vm)?;
                let p = vm.get_prop(&n, "timersPromises")?;
                with_default(vm, p)?
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

fn crypto_pbkdf2(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let alg = vm.to_string(&arg(args, 0))?;
    let password = crypto_input(vm, &arg(args, 1), "password")?;
    let salt = crypto_input(vm, &arg(args, 2), "salt")?;
    let iterations = vm.to_number(&arg(args, 3))?;
    let keylen = vm.to_number(&arg(args, 4))?;
    if !(1.0..=2147483647.0).contains(&iterations) || iterations.fract() != 0.0 {
        return Err(vm.range_error(
            "The value of \"iterations\" is out of range. It must be >= 1 && <= 2147483647",
        ));
    }
    if !(0.0..=2147483647.0).contains(&keylen) || keylen.fract() != 0.0 {
        return Err(vm.range_error(
            "The value of \"keylen\" is out of range. It must be >= 0 && <= 2147483647",
        ));
    }
    let keylen = keylen as usize;
    let iterations = iterations as usize;
    let mut out = Vec::with_capacity(keylen);
    let mut block: u32 = 1;
    while out.len() < keylen {
        let mut msg = salt.clone();
        msg.extend_from_slice(&block.to_be_bytes());
        let Some(mut u) = super::binary::hmac_bytes(&alg, &password, &msg) else {
            return Err(vm.make_error(
                ErrorKind::Error,
                format!("Digest method not supported: {alg}"),
            ));
        };
        let mut t = u.clone();
        for _ in 1..iterations {
            u = super::binary::hmac_bytes(&alg, &password, &u).unwrap_or_default();
            for (a, b) in t.iter_mut().zip(&u) {
                *a ^= b;
            }
        }
        out.extend_from_slice(&t);
        block += 1;
    }
    out.truncate(keylen);
    Ok(super::typed::new_uint8(vm, out))
}

fn crypto_hkdf(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let alg = vm.to_string(&arg(args, 0))?;
    let ikm = crypto_input(vm, &arg(args, 1), "ikm")?;
    let salt = crypto_input(vm, &arg(args, 2), "salt")?;
    let info = crypto_input(vm, &arg(args, 3), "info")?;
    let keylen = vm.to_number(&arg(args, 4))?;
    let Some(hash_len) = super::binary::digest_bytes(&alg, b"").map(|d| d.len()) else {
        return Err(vm.make_error(
            ErrorKind::Error,
            format!("Digest method not supported: {alg}"),
        ));
    };
    if !(keylen >= 0.0 && keylen <= (255 * hash_len) as f64 && keylen.fract() == 0.0) {
        return Err(vm.range_error(format!(
            "The value of \"keylen\" is out of range. It must be >= 0 && <= {}",
            255 * hash_len
        )));
    }
    let keylen = keylen as usize;
    let salt = if salt.is_empty() {
        vec![0u8; hash_len]
    } else {
        salt
    };
    let prk = super::binary::hmac_bytes(&alg, &salt, &ikm).unwrap_or_default();
    let mut out = Vec::with_capacity(keylen);
    let mut previous: Vec<u8> = Vec::new();
    let mut counter: u8 = 1;
    while out.len() < keylen {
        let mut msg = previous.clone();
        msg.extend_from_slice(&info);
        msg.push(counter);
        previous = super::binary::hmac_bytes(&alg, &prk, &msg).unwrap_or_default();
        out.extend_from_slice(&previous);
        counter = counter.wrapping_add(1);
    }
    out.truncate(keylen);
    Ok(super::typed::new_uint8(vm, out))
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

pub const BUILTIN_MODULES: [&str; 55] = [
    "assert",
    "assert/strict",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "diagnostics_channel",
    "dns",
    "dns/promises",
    "domain",
    "events",
    "fs",
    "fs/promises",
    "http",
    "http2",
    "https",
    "inspector",
    "inspector/promises",
    "module",
    "net",
    "os",
    "path",
    "path/posix",
    "path/win32",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "readline/promises",
    "repl",
    "sea",
    "stream",
    "stream/consumers",
    "stream/promises",
    "stream/web",
    "string_decoder",
    "sys",
    "test",
    "timers",
    "timers/promises",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "util/types",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];

pub const PREFIX_ONLY: [&str; 2] = ["sea", "test"];

fn module_create_require(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(vm.globals.get("require").cloned().unwrap_or_default())
}

fn module_is_builtin(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let name = vm.to_string(&arg(args, 0))?;
    let bare = name.strip_prefix("node:").unwrap_or(&name);
    Ok(Value::Bool(BUILTIN_MODULES.contains(&bare)))
}

fn native_format(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(super::system::format_args(vm, args)?.into())
}

fn dns_lookup(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    use std::net::ToSocketAddrs;
    let host = vm.to_string(&arg(args, 0))?;
    let addrs = match (host.as_ref(), 0u16).to_socket_addrs() {
        Ok(a) => a,
        Err(e) => {
            let err = vm.make_error(ErrorKind::Error, format!("getaddrinfo ENOTFOUND {host}"));
            if let Value::Object(o) = &err {
                o.set_own("code", "ENOTFOUND".into());
                o.set_own("errno", Value::Number(-3008.0));
                o.set_own("syscall", "getaddrinfo".into());
                o.set_own("hostname", Value::Str(host.clone()));
                o.set_own("cause", e.to_string().into());
            }
            return Err(err);
        }
    };
    let mut out = Vec::new();
    let mut seen = Vec::new();
    for a in addrs {
        let ip = a.ip().to_string();
        if seen.contains(&ip) {
            continue;
        }
        seen.push(ip.clone());
        let family = if a.is_ipv4() { 4.0 } else { 6.0 };
        let pair = vm.new_array(vec![Value::Str(ip.into()), Value::Number(family)]);
        out.push(pair);
    }
    Ok(vm.new_array(out))
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
