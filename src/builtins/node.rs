use super::binary::{buffer_source_bytes, new_uint8};
use super::{arg, method, native_fn};
use crate::compiler::Mode;
use crate::io::IoEvent;
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use std::io::Read;
use std::path::Path;

type R = Result<Value, Value>;

pub const LAZY_GLOBALS: [&str; 3] = ["Buffer", "WebSocket", "CryptoKey"];

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
      const e = new TypeError(`The first argument must be of type string or an instance of Buffer, ArrayBuffer, or Array or an Array-like Object. Received ${describeReceived(value)}`)
      e.code = "ERR_INVALID_ARG_TYPE"
      throw e
    }
    static #wrap(u8) {
      return new Buffer(u8.buffer, u8.byteOffset, u8.byteLength)
    }
    static alloc(size, fill, encoding) {
      checkSize(size)
      const b = new Buffer(size)
      if (fill !== undefined && fill !== 0) b.fill(fill, 0, b.length, encoding)
      return b
    }
    static allocUnsafe(size) {
      checkSize(size)
      return new Buffer(size)
    }
    static allocUnsafeSlow(size) {
      checkSize(size)
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
    #view(o, size) {
      if (typeof o !== "number") {
        const e = new TypeError(`The "offset" argument must be of type number. Received ${describeReceived(o)}`)
        e.code = "ERR_INVALID_ARG_TYPE"
        throw e
      }
      if (!Number.isInteger(o)) {
        const e = new RangeError(`The value of "offset" is out of range. It must be an integer. Received ${o}`)
        e.code = "ERR_OUT_OF_RANGE"
        throw e
      }
      if (o < 0 || o + size > this.length) {
        const e = new RangeError(`The value of "offset" is out of range. It must be >= 0 and <= ${this.length - size}. Received ${o}`)
        e.code = "ERR_OUT_OF_RANGE"
        throw e
      }
      return new DataView(this.buffer, this.byteOffset, this.byteLength)
    }
    readUInt8(o = 0) { return this.#view(o, 1).getUint8(o) }
    readInt8(o = 0) { return this.#view(o, 1).getInt8(o) }
    readUInt16LE(o = 0) { return this.#view(o, 2).getUint16(o, true) }
    readUInt16BE(o = 0) { return this.#view(o, 2).getUint16(o) }
    readInt16LE(o = 0) { return this.#view(o, 2).getInt16(o, true) }
    readInt16BE(o = 0) { return this.#view(o, 2).getInt16(o) }
    readUInt32LE(o = 0) { return this.#view(o, 4).getUint32(o, true) }
    readUInt32BE(o = 0) { return this.#view(o, 4).getUint32(o) }
    readInt32LE(o = 0) { return this.#view(o, 4).getInt32(o, true) }
    readInt32BE(o = 0) { return this.#view(o, 4).getInt32(o) }
    readFloatLE(o = 0) { return this.#view(o, 4).getFloat32(o, true) }
    readFloatBE(o = 0) { return this.#view(o, 4).getFloat32(o) }
    readDoubleLE(o = 0) { return this.#view(o, 8).getFloat64(o, true) }
    readDoubleBE(o = 0) { return this.#view(o, 8).getFloat64(o) }
    writeUInt8(v, o = 0) { this.#view(o, 1).setUint8(o, v); return o + 1 }
    writeInt8(v, o = 0) { this.#view(o, 1).setInt8(o, v); return o + 1 }
    writeUInt16LE(v, o = 0) { this.#view(o, 2).setUint16(o, v, true); return o + 2 }
    writeUInt16BE(v, o = 0) { this.#view(o, 2).setUint16(o, v); return o + 2 }
    writeInt16LE(v, o = 0) { this.#view(o, 2).setInt16(o, v, true); return o + 2 }
    writeInt16BE(v, o = 0) { this.#view(o, 2).setInt16(o, v); return o + 2 }
    writeUInt32LE(v, o = 0) { this.#view(o, 4).setUint32(o, v, true); return o + 4 }
    writeUInt32BE(v, o = 0) { this.#view(o, 4).setUint32(o, v); return o + 4 }
    writeInt32LE(v, o = 0) { this.#view(o, 4).setInt32(o, v, true); return o + 4 }
    writeInt32BE(v, o = 0) { this.#view(o, 4).setInt32(o, v); return o + 4 }
    writeFloatLE(v, o = 0) { this.#view(o, 4).setFloat32(o, v, true); return o + 4 }
    writeFloatBE(v, o = 0) { this.#view(o, 4).setFloat32(o, v); return o + 4 }
    writeDoubleLE(v, o = 0) { this.#view(o, 8).setFloat64(o, v, true); return o + 8 }
    writeDoubleBE(v, o = 0) { this.#view(o, 8).setFloat64(o, v); return o + 8 }
  }
  const describeReceived = (v) => {
    if (v === null) return "null"
    if (v === undefined) return "undefined"
    if (typeof v === "function") return `function ${v.name}`
    if (typeof v === "object") return v.constructor?.name ? `an instance of ${v.constructor.name}` : inspectLike(v)
    const text = typeof v === "string" && v.length > 28 ? `${v.slice(0, 25)}...` : String(v)
    return `type ${typeof v} (${typeof v === "string" ? `'${text}'` : text})`
  }
  const inspectLike = (v) => {
    try {
      return JSON.stringify(v)
    } catch {
      return String(v)
    }
  }
  const checkSize = (size) => {
    if (typeof size !== "number") {
      const e = new TypeError(`The "size" argument must be of type number. Received ${describeReceived(size)}`)
      e.code = "ERR_INVALID_ARG_TYPE"
      throw e
    }
    if (!(size >= 0 && size <= 9007199254740991)) {
      const e = new RangeError(`The value of "size" is out of range. It must be >= 0 && <= 9007199254740991. Received ${size}`)
      e.code = "ERR_OUT_OF_RANGE"
      throw e
    }
  }
  for (const key of ["from", "alloc", "allocUnsafe", "allocUnsafeSlow", "byteLength", "concat", "isBuffer", "isEncoding", "compare"]) {
    const impl = Buffer[key]
    let value = impl
    if (key === "alloc" || key === "allocUnsafe" || key === "allocUnsafeSlow") {
      value = function (size, fill, encoding) {
        return impl(size, fill, encoding)
      }
      Object.defineProperty(value, "name", { value: key })
    }
    Object.defineProperty(Buffer, key, { value, writable: true, enumerable: true, configurable: true })
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
  EventEmitter.addAbortListener = (signal, listener) => {
    if (typeof listener !== "function") throw new TypeError('The "listener" argument must be of type function')
    let active = true
    if (signal.aborted) queueMicrotask(() => active && listener(signal.reason))
    else signal.addEventListener("abort", listener, { once: true })
    return {
      [Symbol.dispose]() {
        active = false
        signal.removeEventListener("abort", listener)
      },
    }
  }
  class EventEmitterAsyncResource extends EventEmitter {
    #resource
    constructor(options) {
      super(typeof options === "object" && options !== null ? options : undefined)
      const name = typeof options === "string" ? options : options?.name ?? new.target.name
      this.#resource = {
        name,
        asyncId: () => 0,
        triggerAsyncId: () => 0,
        emitDestroy() {},
        runInAsyncScope: (fn, thisArg, ...args) => fn.apply(thisArg, args),
        bind: (fn) => fn,
      }
    }
    get asyncResource() {
      return this.#resource
    }
    get asyncId() {
      return 0
    }
    get triggerAsyncId() {
      return 0
    }
    emitDestroy() {}
  }
  EventEmitter.EventEmitterAsyncResource = EventEmitterAsyncResource
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
    openSync(path, flags = "r", mode) {
      return native.fsOpen(pathOf(path), flags, mode)
    },
    closeSync(fd) {
      native.fsClose(fd)
    },
    readSync(fd, buffer, offset, length, position) {
      if (typeof offset === "object" && offset !== null) [offset, length, position] = [offset.offset, offset.length, offset.position]
      if (typeof position === "bigint") position = Number(position)
      return native.fsReadFd(fd, buffer, offset ?? 0, length ?? buffer.byteLength - (offset ?? 0), position ?? null)
    },
    writeSync(fd, data, offsetOrPosition, lengthOrEncoding, position) {
      if (typeof data === "string") {
        const encoding = typeof lengthOrEncoding === "string" ? lengthOrEncoding : "utf8"
        const buf = Buffer.from(data, encoding)
        return native.fsWriteFd(fd, buf, 0, buf.length, typeof offsetOrPosition === "number" ? offsetOrPosition : null)
      }
      let offset = offsetOrPosition
      let length = lengthOrEncoding
      if (typeof offset === "object" && offset !== null) [offset, length, position] = [offset.offset, offset.length, offset.position]
      if (typeof position === "bigint") position = Number(position)
      return native.fsWriteFd(fd, data, offset ?? 0, length ?? data.byteLength - (offset ?? 0), position ?? null)
    },
    fstatSync(fd, options) {
      return new Stats(native.fsFstat(fd))
    },
    ftruncateSync(fd, len = 0) {
      native.fsFtruncate(fd, len)
    },
    fsyncSync(fd) {
      native.fsFsync(fd)
    },
    fdatasyncSync(fd) {
      native.fsFsync(fd)
    },
    fchmodSync() {},
    fchownSync() {},
    futimesSync() {},
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
  fs.read = (fd, buffer, offset, length, position, cb) => {
    if (typeof buffer === "function") [buffer, offset, length, position, cb] = [Buffer.alloc(16384), 0, 16384, null, buffer]
    else if (typeof offset === "function") [offset, length, position, cb] = [0, buffer.byteLength, null, offset]
    else if (typeof offset === "object" && offset !== null && typeof length === "function") [cb, length, position, offset] = [length, offset.length, offset.position, offset.offset]
    else if (typeof length === "function") [length, position, cb] = [buffer.byteLength - offset, null, length]
    else if (typeof position === "function") [position, cb] = [null, position]
    let n
    let error = null
    try {
      n = fs.readSync(fd, buffer, offset, length, position)
    } catch (e) {
      error = e
    }
    queueMicrotask(() => (error ? cb(error) : cb(null, n, buffer)))
  }
  fs.write = (fd, data, ...rest) => {
    const cb = typeof rest[rest.length - 1] === "function" ? rest.pop() : null
    let n
    let error = null
    try {
      n = fs.writeSync(fd, data, ...rest)
    } catch (e) {
      error = e
    }
    if (cb) queueMicrotask(() => (error ? cb(error) : cb(null, n, data)))
  }
  class FileHandle extends EventEmitter {
    #fd
    constructor(fd) {
      super()
      this.#fd = fd
    }
    get fd() {
      return this.#fd
    }
    #check() {
      if (this.#fd < 0) {
        const e = new Error("file closed")
        e.code = "EBADF"
        throw e
      }
    }
    async read(buffer, offset, length, position) {
      this.#check()
      if (buffer == null || !ArrayBuffer.isView(buffer)) {
        const options = buffer ?? {}
        buffer = options.buffer ?? Buffer.alloc(16384)
        offset = options.offset ?? 0
        length = options.length ?? buffer.byteLength - offset
        position = options.position ?? null
      }
      const bytesRead = fs.readSync(this.#fd, buffer, offset ?? 0, length ?? buffer.byteLength - (offset ?? 0), position ?? null)
      return { bytesRead, buffer }
    }
    async write(data, ...rest) {
      this.#check()
      const bytesWritten = fs.writeSync(this.#fd, data, ...rest)
      return { bytesWritten, buffer: data }
    }
    async readFile(options) {
      this.#check()
      const encoding = typeof options === "string" ? options : options?.encoding
      const chunks = []
      while (true) {
        const buf = Buffer.alloc(65536)
        const n = fs.readSync(this.#fd, buf, 0, buf.length, null)
        if (n === 0) break
        chunks.push(buf.subarray(0, n))
      }
      const all = Buffer.concat(chunks)
      return encoding ? all.toString(encoding) : all
    }
    async writeFile(data, options) {
      this.#check()
      const encoding = typeof options === "string" ? options : options?.encoding ?? "utf8"
      const buf = typeof data === "string" ? Buffer.from(data, encoding) : data
      fs.writeSync(this.#fd, buf, 0, buf.byteLength, null)
    }
    async appendFile(data, options) {
      return this.writeFile(data, options)
    }
    async stat(options) {
      this.#check()
      return fs.fstatSync(this.#fd, options)
    }
    async truncate(len = 0) {
      this.#check()
      fs.ftruncateSync(this.#fd, len)
    }
    async sync() {
      this.#check()
      fs.fsyncSync(this.#fd)
    }
    async datasync() {
      return this.sync()
    }
    async chmod() {}
    async chown() {}
    async utimes() {}
    createReadStream(options = {}) {
      const fd = this.#fd
      let position = options.start ?? null
      const end = options.end ?? Infinity
      const highWaterMark = options.highWaterMark ?? 65536
      const stream = new Readable({
        highWaterMark,
        encoding: options.encoding,
        read() {
          if (position !== null && position > end) return this.push(null)
          const want = Math.min(highWaterMark, position === null ? highWaterMark : end - position + 1)
          const buf = Buffer.alloc(want)
          const n = fs.readSync(fd, buf, 0, want, position)
          if (position !== null) position += n
          if (n === 0) this.push(null)
          else this.push(buf.subarray(0, n))
        },
      })
      stream.fd = fd
      stream.bytesRead = 0
      return stream
    }
    createWriteStream(options = {}) {
      const fd = this.#fd
      let position = options.start ?? null
      const stream = new Writable({
        write(chunk, encoding, cb) {
          const buf = typeof chunk === "string" ? Buffer.from(chunk, encoding) : chunk
          fs.writeSync(fd, buf, 0, buf.byteLength, position)
          if (position !== null) position += buf.byteLength
          cb()
        },
      })
      stream.fd = fd
      return stream
    }
    readLines(options) {
      return createInterface({ input: this.createReadStream(options), crlfDelay: Infinity })
    }
    async close() {
      if (this.#fd < 0) return
      const fd = this.#fd
      this.#fd = -1
      fs.closeSync(fd)
      this.emit("close")
    }
    async [Symbol.asyncDispose]() {
      return this.close()
    }
  }
  promises.open = async (path, flags = "r", mode) => new FileHandle(fs.openSync(path, flags, mode))
  promises.FileHandle = FileHandle
  class FSWatcher extends EventEmitter {
    #id = -1
    #closed = false
    constructor(path, options, listener) {
      super()
      const encoding = options?.encoding ?? "utf8"
      if (typeof listener === "function") this.on("change", listener)
      this.#id = native.watchStart(pathOf(path), !!options?.recursive, options?.interval ?? 200, (kind, filename) => {
        if (this.#closed) return
        const name = filename == null ? null : encoding === "buffer" ? Buffer.from(filename) : filename
        this.emit("change", kind, name)
      })
      if (options?.persistent === false) this.unref()
      if (options?.signal) options.signal.addEventListener("abort", () => this.close(), { once: true })
    }
    close() {
      if (this.#closed) return
      this.#closed = true
      native.watchClose(this.#id)
      queueMicrotask(() => this.emit("close"))
    }
    ref() {
      if (!this.#closed) native.watchRef(this.#id, true)
      return this
    }
    unref() {
      if (!this.#closed) native.watchRef(this.#id, false)
      return this
    }
    [Symbol.asyncDispose]() {
      this.close()
      return Promise.resolve()
    }
  }
  fs.watch = (path, options, listener) => {
    if (typeof options === "function") [options, listener] = [undefined, options]
    if (typeof options === "string") options = { encoding: options }
    return new FSWatcher(path, options, listener)
  }
  fs.FSWatcher = FSWatcher
  const emptyStats = () => new Stats({ mode: 0, size: 0, atimeMs: 0, mtimeMs: 0, ctimeMs: 0, birthtimeMs: 0, kind: "file" })
  const statOrEmpty = (p) => {
    try {
      return fs.statSync(p)
    } catch {
      return emptyStats()
    }
  }
  class StatWatcher extends EventEmitter {
    #timer = null
    #prev
    constructor(path, options) {
      super()
      this.#prev = statOrEmpty(path)
      this.#timer = setInterval(() => {
        const curr = statOrEmpty(path)
        if (curr.mtimeMs !== this.#prev.mtimeMs || curr.size !== this.#prev.size || curr.mode !== this.#prev.mode) {
          const prev = this.#prev
          this.#prev = curr
          this.emit("change", curr, prev)
        }
      }, options?.interval ?? 5007)
      if (options?.persistent === false) this.#timer.unref()
    }
    stop() {
      if (this.#timer === null) return
      clearInterval(this.#timer)
      this.#timer = null
      queueMicrotask(() => this.emit("stop"))
    }
    ref() {
      this.#timer?.ref()
      return this
    }
    unref() {
      this.#timer?.unref()
      return this
    }
  }
  fs.StatWatcher = StatWatcher
  const statWatchers = new Map()
  fs.watchFile = (path, options, listener) => {
    if (typeof options === "function") [options, listener] = [undefined, options]
    if (typeof listener !== "function") throw new TypeError('The "listener" argument must be of type function')
    const key = pathOf(path)
    let watcher = statWatchers.get(key)
    if (!watcher) {
      watcher = new StatWatcher(key, options)
      statWatchers.set(key, watcher)
    }
    watcher.on("change", listener)
    return watcher
  }
  fs.unwatchFile = (path, listener) => {
    const key = pathOf(path)
    const watcher = statWatchers.get(key)
    if (!watcher) return
    if (typeof listener === "function") watcher.removeListener("change", listener)
    else watcher.removeAllListeners("change")
    if (watcher.listenerCount("change") === 0) {
      watcher.stop()
      statWatchers.delete(key)
    }
  }
  promises.watch = (path, options = {}) => {
    const watcher = new FSWatcher(path, options)
    const queue = []
    let wake = null
    let done = false
    let failure = null
    const notify = () => {
      if (wake) {
        const w = wake
        wake = null
        w()
      }
    }
    watcher.on("change", (eventType, filename) => {
      queue.push({ eventType, filename })
      notify()
    })
    watcher.on("error", (e) => {
      failure = e
      notify()
    })
    watcher.on("close", () => {
      done = true
      notify()
    })
    return {
      [Symbol.asyncIterator]() {
        return this
      },
      async next() {
        while (queue.length === 0 && !done && !failure) await new Promise((resolve) => (wake = resolve))
        if (failure) throw failure
        if (queue.length === 0) return { value: undefined, done: true }
        return { value: queue.shift(), done: false }
      },
      async return() {
        watcher.close()
        return { value: undefined, done: true }
      },
    }
  }
  promises.constants = fs.constants
  fs.promises = promises
  fs.realpath.native = fs.realpath
  fs.realpathSync.native = fs.realpathSync
  Object.assign(fs.constants, { O_RDONLY: 0, O_WRONLY: 1, O_RDWR: 2, O_CREAT: 64, O_EXCL: 128, O_NOCTTY: 256, O_TRUNC: 512, O_APPEND: 1024, O_DIRECTORY: 65536, O_NOFOLLOW: 131072, O_SYNC: 1052672, O_DSYNC: 4096, O_NONBLOCK: 2048, S_IFMT: 61440, S_IFREG: 32768, S_IFDIR: 16384, S_IFCHR: 8192, S_IFBLK: 24576, S_IFIFO: 4096, S_IFLNK: 40960, S_IFSOCK: 49152, S_IRWXU: 448, S_IRUSR: 256, S_IWUSR: 128, S_IXUSR: 64, S_IRWXG: 56, S_IRGRP: 32, S_IWGRP: 16, S_IXGRP: 8, S_IRWXO: 7, S_IROTH: 4, S_IWOTH: 2, S_IXOTH: 1, UV_FS_O_FILEMAP: 0, COPYFILE_EXCL: 1, COPYFILE_FICLONE: 2, COPYFILE_FICLONE_FORCE: 4 })

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

  const kPromisifyCustom = Symbol.for("nodejs.util.promisify.custom")
  function promisify(fn) {
    if (typeof fn !== "function") throw new TypeError('The "original" argument must be of type function')
    if (typeof fn[kPromisifyCustom] === "function") return fn[kPromisifyCustom]
    return function (...args) {
      return new Promise((resolve, reject) => {
        fn.call(this, ...args, (err, ...values) => {
          if (err) reject(err)
          else resolve(values.length > 1 ? values : values[0])
        })
      })
    }
  }

  promisify.custom = kPromisifyCustom

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
    isProxy: (v) => native.isProxy(v),
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
  if (native.isTTY(0)) stdin.isTTY = true
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
      this.readableEnded = false
      this.closed = false
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
          this.readableEnded = true
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
    get destroyed() {
      return this.#destroyed
    }
    destroy(err) {
      if (this.#destroyed) return this
      this.#destroyed = true
      this.readable = false
      this.#chunks = []
      queueMicrotask(() => {
        if (err) this.emit("error", err)
        this.closed = true
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
      this.writableEnded = false
      this.writableFinished = false
      this.closed = false
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
        this.writableEnded = true
        queueMicrotask(() => {
          this.writableFinished = true
          this.emit("finish")
          this.destroy()
        })
      }
      return this
    }
    get destroyed() {
      return this.#destroyed
    }
    destroy(err) {
      if (this.#destroyed) return this
      this.#destroyed = true
      this.writable = false
      queueMicrotask(() => {
        if (err) this.emit("error", err)
        this.closed = true
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
    const text = String(s).replace(/\+/g, " ")
    try {
      return decodeURIComponent(text)
    } catch {}
    return text.replace(/(?:%[0-9a-fA-F]{2})+/g, (m) => {
      try {
        return decodeURIComponent(m)
      } catch {
        return m
      }
    })
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
  const makeHistogram = () => ({
    count: 0,
    countBigInt: 0n,
    min: 0,
    minBigInt: 0n,
    max: 0,
    maxBigInt: 0n,
    mean: 0,
    stddev: 0,
    exceeds: 0,
    exceedsBigInt: 0n,
    percentiles: new Map(),
    percentilesBigInt: new Map(),
    percentile: () => 0,
    percentileBigInt: () => 0n,
    record() {},
    recordDelta() {},
    add() {},
    reset() {},
    enable: () => true,
    disable: () => true,
  })
  const perfHooksExtras = {
    createHistogram: () => makeHistogram(),
    monitorEventLoopDelay: () => makeHistogram(),
    constants: { NODE_PERFORMANCE_GC_MAJOR: 4, NODE_PERFORMANCE_GC_MINOR: 1, NODE_PERFORMANCE_GC_INCREMENTAL: 8, NODE_PERFORMANCE_GC_WEAKCB: 16, NODE_PERFORMANCE_GC_FLAGS_NO: 0, NODE_PERFORMANCE_GC_FLAGS_FORCED: 4 },
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
        if (r.decoder) {
          const tail = r.decoder.end()
          if (tail) r.buffer.push(tail)
        }
        maybeEndRead(this)
        return false
      }
      let c = toChunk(this, chunk, encoding, r.objectMode)
      if (!r.objectMode && r.encoding && typeof c !== "string") {
        r.decoder ??= new StringDecoder(r.encoding)
        c = r.decoder.write(c)
        if (c === "") {
          scheduleRead(this)
          return this.readableLength < r.highWaterMark
        }
      }
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
      r.decoder = new StringDecoder(r.encoding)
      r.buffer = r.buffer.map((c) => (typeof c === "string" ? c : r.decoder.write(c))).filter((c) => c !== "")
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
    map(fn, options) {
      const src = this
      return Readable.from((async function* () {
        for await (const c of src) yield await fn(c, { signal: options?.signal })
      })())
    },
    filter(fn, options) {
      const src = this
      return Readable.from((async function* () {
        for await (const c of src) if (await fn(c, { signal: options?.signal })) yield c
      })())
    },
    flatMap(fn, options) {
      const src = this
      return Readable.from((async function* () {
        for await (const c of src) {
          const r = await fn(c, { signal: options?.signal })
          if (r !== null && typeof r === "object" && (typeof r[Symbol.asyncIterator] === "function" || typeof r[Symbol.iterator] === "function")) {
            for await (const x of r) yield x
          } else yield r
        }
      })())
    },
    take(n) {
      const src = this
      return Readable.from((async function* () {
        let count = 0
        if (n <= 0) return
        for await (const c of src) {
          yield c
          if (++count >= n) return
        }
      })())
    },
    drop(n) {
      const src = this
      return Readable.from((async function* () {
        let count = 0
        for await (const c of src) if (count++ >= n) yield c
      })())
    },
    async forEach(fn, options) {
      for await (const c of this) await fn(c, { signal: options?.signal })
    },
    async reduce(fn, initial) {
      let acc = initial
      let first = arguments.length < 2
      for await (const c of this) {
        if (first) {
          acc = c
          first = false
        } else acc = await fn(acc, c)
      }
      if (first) throw new TypeError("Reduce of an empty stream requires an initial value")
      return acc
    },
    async some(fn, options) {
      for await (const c of this) if (await fn(c, { signal: options?.signal })) return true
      return false
    },
    async every(fn, options) {
      for await (const c of this) if (!(await fn(c, { signal: options?.signal }))) return false
      return true
    },
    async find(fn, options) {
      for await (const c of this) if (await fn(c, { signal: options?.signal })) return c
      return undefined
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
      const buffered = w.queue.reduce((n, [chunk]) => n + (w.objectMode ? 1 : chunk.length), 0) + (w.writing && w.current ? (w.objectMode ? 1 : w.current.length) : 0)
      const ok = buffered < w.highWaterMark
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
    w.current = chunk
    let called = false
    const done = (err) => {
      if (called) return
      called = true
      w.writing = false
      w.current = null
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
  Object.defineProperty(Writable, Symbol.hasInstance, {
    value(object) {
      if (Function.prototype[Symbol.hasInstance].call(this, object)) return true
      return this === Writable && object !== null && typeof object === "object" && object[kWrite] !== undefined
    },
    configurable: true,
  })

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
    const last = streams[streams.length - 1]
    finished(last, last[kWrite] !== undefined ? { readable: false } : {}, (err) => settle(err))
    return last
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
  const defaultHighWaterMarks = { object: 16, bytes: 65536 }
  stream.getDefaultHighWaterMark = (objectMode) => (objectMode ? defaultHighWaterMarks.object : defaultHighWaterMarks.bytes)
  stream.setDefaultHighWaterMark = (objectMode, value) => {
    if (objectMode) defaultHighWaterMarks.object = value
    else defaultHighWaterMarks.bytes = value
  }
  stream.isReadable = (s) => !!(s && s[kRead] && !s[kRead].destroyed && !s[kRead].endEmitted)
  stream.isWritable = (s) => !!(s && s[kWrite] && !s[kWrite].destroyed && !s[kWrite].ending)
  stream.isErrored = () => false
  Readable.toWeb = (r) =>
    new ReadableStream({
      start(controller) {
        r.on("data", (chunk) => controller.enqueue(chunk))
        r.on("end", () => controller.close())
        r.on("error", (e) => controller.error(e))
      },
      cancel() {
        r.destroy()
      },
    })
  Readable.fromWeb = (rs, options) => Readable.from(rs, options)
  Writable.toWeb = (w) =>
    new WritableStream({
      write(chunk) {
        return new Promise((resolve, reject) => w.write(chunk, (e) => (e ? reject(e) : resolve())))
      },
      close() {
        return new Promise((resolve) => w.end(resolve))
      },
      abort(reason) {
        w.destroy(reason)
      },
    })
  Writable.fromWeb = (ws, options) => {
    const writer = ws.getWriter()
    return new Writable({
      ...options,
      write(chunk, encoding, cb) {
        writer.write(chunk).then(() => cb(), cb)
      },
      final(cb) {
        writer.close().then(() => cb(), cb)
      },
    })
  }
  Duplex.toWeb = (d) => ({ readable: Readable.toWeb(d), writable: Writable.toWeb(d) })
  Duplex.fromWeb = (pair, options) => {
    const writer = pair.writable.getWriter()
    const d = new Duplex({
      ...options,
      read() {},
      write(chunk, encoding, cb) {
        writer.write(chunk).then(() => cb(), cb)
      },
      final(cb) {
        writer.close().then(() => cb(), cb)
      },
    })
    ;(async () => {
      try {
        for await (const chunk of pair.readable) d.push(chunk)
        d.push(null)
      } catch (e) {
        d.destroy(e)
      }
    })()
    return d
  }
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
      this.trailers = {}
      this.rawTrailers = []
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
      super({ highWaterMark: Infinity, autoDestroy: false })
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
    _destroy(err, cb) {
      if (this.socket && !this.socket.destroyed) this.socket.destroy()
      this.#controller.abort()
      cb(err)
    }
    #rawRequest(body, cb) {
      const secure = this.#url.protocol === "https:" || this.#url.protocol === "wss:"
      const port = Number(this.#url.port) || (secure ? 443 : 80)
      const host = this.#url.hostname.replace(/^\[|\]$/g, "")
      const agentOptions = this.#options.agent?.options ?? {}
      const sock = secure ? tls.connect({ host, port, servername: this.#options.servername ?? host, rejectUnauthorized: this.#options.rejectUnauthorized ?? agentOptions.rejectUnauthorized, ca: this.#options.ca ?? agentOptions.ca }) : net.connect({ host, port })
      this.socket = sock
      this.connection = sock
      this.emit("socket", sock)
      sock.once(secure ? "secureConnect" : "connect", () => {
        if (!this.#headers.has("host")) this.setHeader("Host", this.#url.host)
        if (body && !this.#headers.has("content-length")) this.setHeader("Content-Length", String(body.length))
        let head = `${this.method} ${this.path} HTTP/1.1\r\n`
        for (const [k, v] of this.#headers) head += `${k}: ${v}\r\n`
        head += "\r\n"
        sock.write(head)
        if (body) sock.write(body)
        cb()
      })
      let buffered = Buffer.alloc(0)
      let headDone = false
      const onData = (chunk) => {
        if (headDone) return
        buffered = Buffer.concat([buffered, chunk])
        const idx = buffered.indexOf("\r\n\r\n")
        if (idx < 0) return
        headDone = true
        sock.removeListener("data", onData)
        const lines = buffered.subarray(0, idx).toString("latin1").split("\r\n")
        const rest = buffered.subarray(idx + 4)
        const [, statusText, ...message] = lines[0].split(" ")
        const statusCode = Number(statusText)
        const headers = {}
        const rawHeaders = []
        for (const line of lines.slice(1)) {
          const i = line.indexOf(":")
          if (i < 0) continue
          const name = line.slice(0, i).trim()
          const key = name.toLowerCase()
          const value = line.slice(i + 1).trim()
          rawHeaders.push(name, value)
          if (headers[key] === undefined) headers[key] = value
          else if (key === "set-cookie") headers[key] = [].concat(headers[key], value)
          else headers[key] += ", " + value
        }
        const res = new Readable({ read() {} })
        Object.assign(res, { statusCode, statusMessage: message.join(" "), headers, rawHeaders, trailers: {}, rawTrailers: [], httpVersion: "1.1", httpVersionMajor: 1, httpVersionMinor: 1, complete: false, socket: sock, req: this, url: "", method: null, aborted: false })
        res.setTimeout = () => res
        this.res = res
        if (statusCode === 101 && this.listenerCount("upgrade") > 0) {
          this.emit("upgrade", res, sock, rest)
          return
        }
        if (rest.length) res.push(rest)
        sock.on("data", (c) => res.push(c))
        sock.on("end", () => {
          res.complete = true
          res.push(null)
        })
        this.emit("response", res)
      }
      sock.on("data", onData)
      sock.on("error", (e) => {
        if (!this.destroyed) this.destroy(e)
      })
      sock.on("close", () => {
        if (!headDone && !this.destroyed) {
          const e = new Error("socket hang up")
          e.code = "ECONNRESET"
          this.destroy(e)
        }
      })
    }
    _final(cb) {
      this.#sent = true
      this.headersSent = true
      const body = this.#chunks.length ? Buffer.concat(this.#chunks) : undefined
      if (this.#headers.has("upgrade") || this.#options.createConnection) return this.#rawRequest(body, cb)
      const agentOptions = this.#options.agent?.options ?? {}
      const tlsOptions = { rejectUnauthorized: this.#options.rejectUnauthorized ?? agentOptions.rejectUnauthorized, ca: this.#options.ca ?? agentOptions.ca }
      fetch(this.#url, { method: this.method, headers: this.#headers, body, signal: this.#controller.signal, redirect: "manual", tls: tlsOptions }).then(
        (res) => {
          const incoming = new IncomingResponse(res)
          incoming.req = this
          this.res = incoming
          this.emit("response", incoming)
          cb()
        },
        (e) => {
          cb()
          if (!this.destroyed) this.destroy(nodeRequestError(e, this.#url))
        },
      )
    }
  }

  const nodeRequestError = (e, url) => {
    if (e?.name === "AbortError") return e
    const detail = String(e?.cause?.message ?? e?.message ?? e)
    const host = url.hostname
    const port = url.port || (url.protocol === "https:" ? "443" : "80")
    const make = (code, message) => Object.assign(new Error(message), { code, cause: e?.cause })
    if (/refused/i.test(detail)) return Object.assign(make("ECONNREFUSED", `connect ECONNREFUSED ${host}:${port}`), { errno: -4078, syscall: "connect", address: host, port: Number(port) })
    if (/self[- ]signed/i.test(detail)) return make("DEPTH_ZERO_SELF_SIGNED_CERT", "self-signed certificate")
    if (/UnknownIssuer|unknown issuer|not valid for name|NotValidForName|certificate/i.test(detail)) return make(/name/i.test(detail) ? "ERR_TLS_CERT_ALTNAME_INVALID" : "UNABLE_TO_VERIFY_LEAF_SIGNATURE", detail)
    if (/dns|resolve|lookup|No such host/i.test(detail)) return Object.assign(make("ENOTFOUND", `getaddrinfo ENOTFOUND ${host}`), { syscall: "getaddrinfo", hostname: host })
    if (/timed? ?out/i.test(detail)) return make("ETIMEDOUT", detail)
    if (/reset|closed/i.test(detail)) return make("ECONNRESET", "socket hang up")
    return make(e?.code ?? "ECONNRESET", detail)
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
  const withHttps = (url, options) => {
    if (typeof url === "object" && url !== null && !(url instanceof URL)) return [url.protocol == null ? { ...url, protocol: "https:" } : url, options]
    if (typeof options === "object" && options !== null && options.protocol == null) return [url, { ...options, protocol: "https:" }]
    return [url, options]
  }
  const httpsClient = {
    ...httpClient,
    request(url, options, cb) {
      const [u, o] = withHttps(url, options)
      return new ClientRequest(u, o, cb)
    },
    get(url, options, cb) {
      const req = httpsClient.request(url, options, cb)
      req.end()
      return req
    },
    globalAgent: new Agent({ protocol: "https:" }),
  }

  const WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
  const wsOrigin = (href) => {
    const u = new URL(href)
    return `${u.protocol === "wss:" ? "https:" : "http:"}//${u.host}`
  }
  class WebSocket extends EventTarget {
    static CONNECTING = 0
    static OPEN = 1
    static CLOSING = 2
    static CLOSED = 3
    #url
    #protocol = ""
    #extensions = ""
    #readyState = 0
    #binaryType = "blob"
    #bufferedAmount = 0
    #socket = null
    #buffer = Buffer.alloc(0)
    #fragments = []
    #fragmentOpcode = 0
    #closeSent = false
    #closeInfo = null
    #closeTimer = null
    #pendingClose = null
    #req = null
    constructor(url, protocols) {
      super()
      this.onopen = null
      this.onmessage = null
      this.onclose = null
      this.onerror = null
      let parsed
      try {
        parsed = new URL(String(url))
      } catch {
        throw new DOMException(`Failed to construct 'WebSocket': The URL '${url}' is invalid.`, "SyntaxError")
      }
      if (parsed.protocol === "http:") parsed.protocol = "ws:"
      else if (parsed.protocol === "https:") parsed.protocol = "wss:"
      if (parsed.protocol !== "ws:" && parsed.protocol !== "wss:") {
        throw new DOMException(`Failed to construct 'WebSocket': The URL's scheme must be either 'http', 'https', 'ws', or 'wss'. '${parsed.protocol}' is not allowed.`, "SyntaxError")
      }
      if (parsed.hash) throw new DOMException("Failed to construct 'WebSocket': The URL contains a fragment identifier.", "SyntaxError")
      this.#url = parsed.href
      const list = protocols === undefined ? [] : Array.isArray(protocols) ? protocols.map(String) : [String(protocols)]
      if (new Set(list).size !== list.length) throw new DOMException("Failed to construct 'WebSocket': The subprotocol list contains duplicates.", "SyntaxError")
      queueMicrotask(() => this.#connect(parsed, list))
    }
    get CONNECTING() {
      return 0
    }
    get OPEN() {
      return 1
    }
    get CLOSING() {
      return 2
    }
    get CLOSED() {
      return 3
    }
    get url() {
      return this.#url
    }
    get readyState() {
      return this.#readyState
    }
    get protocol() {
      return this.#protocol
    }
    get extensions() {
      return this.#extensions
    }
    get bufferedAmount() {
      return this.#bufferedAmount
    }
    get binaryType() {
      return this.#binaryType
    }
    set binaryType(value) {
      if (value === "blob" || value === "arraybuffer" || value === "nodebuffer") this.#binaryType = value
    }
    #connect(u, protocols) {
      const key = Buffer.from(native.randomBytes(16).buffer).toString("base64")
      const secure = u.protocol === "wss:"
      const headers = { Host: u.host, Upgrade: "websocket", Connection: "Upgrade", "Sec-WebSocket-Key": key, "Sec-WebSocket-Version": "13" }
      if (protocols.length) headers["Sec-WebSocket-Protocol"] = protocols.join(", ")
      const req = (secure ? httpsClient : httpClient).request({ protocol: secure ? "https:" : "http:", hostname: u.hostname, port: u.port || (secure ? 443 : 80), path: u.pathname + u.search, headers })
      this.#req = req
      req.on("upgrade", (res, socket, head) => {
        const expected = crypto.createHash("sha1").update(key + WS_GUID).digest("base64")
        if (res.headers["sec-websocket-accept"] !== expected) {
          socket.destroy()
          return this.#fail("Invalid Sec-WebSocket-Accept header")
        }
        const chosen = res.headers["sec-websocket-protocol"]
        if (chosen !== undefined && !protocols.includes(chosen)) {
          socket.destroy()
          return this.#fail("Server sent a subprotocol that was not requested")
        }
        if (res.headers["sec-websocket-extensions"]) {
          socket.destroy()
          return this.#fail("Server sent an extension that was not requested")
        }
        this.#protocol = chosen ?? ""
        this.#socket = socket
        this.#readyState = 1
        socket.on("data", (chunk) => this.#onData(chunk))
        socket.on("close", () => this.#onSocketClose())
        socket.on("error", () => {})
        if (head.length) this.#onData(head)
        if (this.#pendingClose) {
          const [code, reason] = this.#pendingClose
          this.#pendingClose = null
          this.#readyState = 1
          this.close(code, reason)
          return
        }
        this.dispatchEvent(new Event("open"))
      })
      req.on("response", (res) => this.#fail(`Unexpected server response: ${res.statusCode}`))
      req.on("error", (e) => this.#fail(e.message))
      req.end()
    }
    #fail(message) {
      if (this.#readyState === 3) return
      this.#readyState = 3
      const error = new Error(message)
      this.dispatchEvent(new ErrorEvent("error", { message, error }))
      this.dispatchEvent(new CloseEvent("close", { wasClean: false, code: 1006, reason: "" }))
    }
    send(data) {
      if (this.#readyState === 0) throw new DOMException("Failed to execute 'send' on 'WebSocket': Still in CONNECTING state.", "InvalidStateError")
      if (this.#readyState !== 1) return
      if (typeof data === "string") return this.#sendFrame(1, Buffer.from(data, "utf8"))
      if (typeof Blob === "function" && data instanceof Blob) {
        this.#bufferedAmount += data.size
        data.arrayBuffer().then((ab) => {
          this.#bufferedAmount -= data.size
          if (this.#readyState === 1) this.#sendFrame(2, Buffer.from(ab))
        })
        return
      }
      if (data instanceof ArrayBuffer) return this.#sendFrame(2, Buffer.from(data))
      if (ArrayBuffer.isView(data)) return this.#sendFrame(2, Buffer.from(data.buffer, data.byteOffset, data.byteLength))
      this.#sendFrame(1, Buffer.from(String(data), "utf8"))
    }
    #sendFrame(opcode, payload) {
      if (!this.#socket || this.#socket.destroyed) return
      const mask = Buffer.from(native.randomBytes(4).buffer)
      const len = payload.length
      let header
      if (len < 126) header = Buffer.from([0x80 | opcode, 0x80 | len])
      else if (len < 65536) {
        header = Buffer.alloc(4)
        header[0] = 0x80 | opcode
        header[1] = 0x80 | 126
        header.writeUInt16BE(len, 2)
      } else {
        header = Buffer.alloc(10)
        header[0] = 0x80 | opcode
        header[1] = 0x80 | 127
        header.writeBigUInt64BE(BigInt(len), 2)
      }
      const masked = Buffer.alloc(len)
      for (let i = 0; i < len; i++) masked[i] = payload[i] ^ mask[i & 3]
      this.#socket.write(Buffer.concat([header, mask, masked]))
    }
    ping(data) {
      if (this.#readyState === 1) this.#sendFrame(9, data === undefined ? Buffer.alloc(0) : Buffer.from(data))
    }
    close(code, reason) {
      if (code !== undefined && code !== 1000 && !(code >= 3000 && code <= 4999)) {
        throw new DOMException("Failed to execute 'close' on 'WebSocket': The close code must be either 1000, or between 3000 and 4999.", "InvalidAccessError")
      }
      const text = reason === undefined ? Buffer.alloc(0) : Buffer.from(String(reason), "utf8")
      if (text.length > 123) throw new DOMException("Failed to execute 'close' on 'WebSocket': The close reason must not be greater than 123 UTF-8 bytes.", "SyntaxError")
      if (this.#readyState === 2 || this.#readyState === 3) return
      if (this.#readyState === 0) {
        this.#readyState = 2
        this.#pendingClose = [code, reason]
        return
      }
      this.#readyState = 2
      const payload = code === undefined ? Buffer.alloc(0) : Buffer.concat([Buffer.from([code >> 8, code & 255]), text])
      this.#sendFrame(8, payload)
      this.#closeSent = true
      this.#closeTimer = setTimeout(() => this.#socket?.destroy(), 30000)
      if (typeof this.#closeTimer?.unref === "function") this.#closeTimer.unref()
    }
    #onData(chunk) {
      this.#buffer = this.#buffer.length ? Buffer.concat([this.#buffer, chunk]) : Buffer.from(chunk.buffer, chunk.byteOffset, chunk.byteLength)
      while (this.#readyState !== 3) {
        const b = this.#buffer
        if (b.length < 2) return
        const fin = (b[0] & 0x80) !== 0
        const rsv = b[0] & 0x70
        const opcode = b[0] & 0x0f
        const masked = (b[1] & 0x80) !== 0
        let len = b[1] & 0x7f
        let offset = 2
        if (len === 126) {
          if (b.length < 4) return
          len = b.readUInt16BE(2)
          offset = 4
        } else if (len === 127) {
          if (b.length < 10) return
          len = Number(b.readBigUInt64BE(2))
          offset = 10
        }
        if (masked) offset += 4
        if (b.length < offset + len) return
        if (rsv) return this.#protocolError("RSV bits are set")
        let payload = b.subarray(offset, offset + len)
        if (masked) {
          const mask = b.subarray(offset - 4, offset)
          const out = Buffer.alloc(len)
          for (let i = 0; i < len; i++) out[i] = payload[i] ^ mask[i & 3]
          payload = out
        }
        this.#buffer = b.subarray(offset + len)
        this.#onFrame(fin, opcode, payload)
      }
    }
    #onFrame(fin, opcode, payload) {
      switch (opcode) {
        case 0:
          if (!this.#fragmentOpcode) return this.#protocolError("Unexpected continuation frame")
          this.#fragments.push(payload)
          if (fin) {
            const data = Buffer.concat(this.#fragments)
            const op = this.#fragmentOpcode
            this.#fragments = []
            this.#fragmentOpcode = 0
            this.#deliver(op, data)
          }
          break
        case 1:
        case 2:
          if (this.#fragmentOpcode) return this.#protocolError("Expected a continuation frame")
          if (fin) this.#deliver(opcode, payload)
          else {
            this.#fragmentOpcode = opcode
            this.#fragments = [payload]
          }
          break
        case 8: {
          let code = 1005
          let reason = ""
          if (payload.length === 1) return this.#protocolError("Invalid close payload")
          if (payload.length >= 2) {
            code = payload.readUInt16BE(0)
            reason = payload.subarray(2).toString("utf8")
          }
          this.#closeInfo = { code, reason, wasClean: true }
          if (!this.#closeSent) {
            this.#readyState = 2
            this.#sendFrame(8, payload.subarray(0, Math.min(payload.length, 125)))
            this.#closeSent = true
          }
          this.#socket.end()
          break
        }
        case 9:
          if (this.#readyState === 1) this.#sendFrame(10, payload)
          break
        case 10:
          break
        default:
          this.#protocolError("Invalid opcode")
      }
    }
    #deliver(opcode, data) {
      let payload
      if (opcode === 1) {
        try {
          payload = new TextDecoder("utf-8", { fatal: true }).decode(data)
        } catch {
          return this.#protocolError("Invalid UTF-8 payload", 1007)
        }
      } else if (this.#binaryType === "arraybuffer") payload = data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength)
      else if (this.#binaryType === "nodebuffer") payload = Buffer.from(data)
      else payload = new Blob([data])
      this.dispatchEvent(new MessageEvent("message", { data: payload, origin: wsOrigin(this.#url) }))
    }
    #protocolError(message, code = 1002) {
      if (this.#readyState === 3) return
      this.#closeInfo = { code, reason: message, wasClean: false }
      if (!this.#closeSent && this.#socket && !this.#socket.destroyed) {
        try {
          this.#sendFrame(8, Buffer.concat([Buffer.from([code >> 8, code & 255]), Buffer.from(message)]))
        } catch {}
        this.#closeSent = true
      }
      this.#readyState = 2
      this.dispatchEvent(new ErrorEvent("error", { message, error: new Error(message) }))
      this.#socket?.destroy()
    }
    #onSocketClose() {
      if (this.#readyState === 3) return
      clearTimeout(this.#closeTimer)
      this.#readyState = 3
      const info = this.#closeInfo ?? { code: 1006, reason: "", wasClean: false }
      this.dispatchEvent(new CloseEvent("close", info))
    }
  }
  Object.defineProperty(WebSocket.prototype, Symbol.toStringTag, { value: "WebSocket", configurable: true })

  const kKeyMaterial = Symbol("keyMaterial")
  const subtleHashName = (h) => {
    const name = typeof h === "string" ? h : h?.name
    const map = { "SHA-1": "sha1", "SHA-256": "sha256", "SHA-384": "sha384", "SHA-512": "sha512" }
    if (!map[name]) throw new DOMException(`Unrecognized algorithm name: ${name}`, "NotSupportedError")
    return map[name]
  }
  const subtleBytes = (data, what = "data") => {
    if (data instanceof ArrayBuffer) return new Uint8Array(data.slice(0))
    if (ArrayBuffer.isView(data)) return new Uint8Array(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength))
    throw new TypeError(`The "${what}" argument must be an instance of ArrayBuffer or ArrayBufferView`)
  }
  const toArrayBuffer = (bytes) => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength)
  const base64urlDecode = (text) => Buffer.from(String(text).replace(/-/g, "+").replace(/_/g, "/"), "base64")
  const base64urlEncode = (bytes) => Buffer.from(bytes).toString("base64url")
  class CryptoKey {
    #material
    constructor(material, algorithm, extractable, usages, type = "secret") {
      this.#material = material
      Object.defineProperty(this, "type", { value: type, enumerable: true })
      Object.defineProperty(this, "extractable", { value: !!extractable, enumerable: true })
      Object.defineProperty(this, "algorithm", { value: Object.freeze(algorithm), enumerable: true })
      Object.defineProperty(this, "usages", { value: Object.freeze([...usages]), enumerable: true })
    }
    get [kKeyMaterial]() {
      return this.#material
    }
    get [Symbol.toStringTag]() {
      return "CryptoKey"
    }
  }
  const normalizeAlgorithm = (alg) => (typeof alg === "string" ? { name: alg } : { ...alg })
  const algName = (alg) => String(normalizeAlgorithm(alg).name).toUpperCase().replace(/^SHA(\d)/, "SHA-$1")
  const requireKey = (key, what = "key") => {
    if (!(key instanceof CryptoKey)) throw new TypeError(`${what} is not a CryptoKey`)
  }
  const requireUsage = (key, usage) => {
    if (!key.usages.includes(usage)) throw new DOMException(`The key does not support the '${usage}' operation`, "InvalidAccessError")
  }
  const hashBlockBits = { sha1: 512, sha256: 512, sha384: 1024, sha512: 1024 }
  const deriveRaw = (algorithm, baseKey, bits) => {
    const alg = normalizeAlgorithm(algorithm)
    const name = algName(alg)
    const material = baseKey[kKeyMaterial]
    if (bits === null || bits === undefined) throw new DOMException("length is required", "OperationError")
    const bytes = Math.ceil(bits / 8)
    if (name === "PBKDF2") {
      return native.pbkdf2(subtleHashName(alg.hash), material, subtleBytes(alg.salt, "salt"), alg.iterations, bytes)
    }
    if (name === "HKDF") {
      return native.hkdf(subtleHashName(alg.hash), material, subtleBytes(alg.salt ?? new Uint8Array(0), "salt"), subtleBytes(alg.info ?? new Uint8Array(0), "info"), bytes)
    }
    throw new DOMException(`Unsupported derivation algorithm: ${name}`, "NotSupportedError")
  }
  const subtleCrypto = {
    async importKey(format, keyData, algorithm, extractable, usages) {
      const alg = normalizeAlgorithm(algorithm)
      const name = algName(alg)
      let material
      if (format === "raw") material = subtleBytes(keyData, "keyData")
      else if (format === "jwk") {
        if (!keyData || keyData.kty !== "oct" || typeof keyData.k !== "string") throw new DOMException("Only symmetric (oct) JSON Web Keys are supported", "DataError")
        material = new Uint8Array(base64urlDecode(keyData.k))
      } else throw new DOMException(`Unsupported key format: ${format}`, "NotSupportedError")
      if (name === "HMAC") {
        const hash = normalizeAlgorithm(alg.hash)
        subtleHashName(hash)
        const length = alg.length ?? material.length * 8
        return new CryptoKey(material, { name: "HMAC", hash: { name: algName(hash) }, length }, extractable, usages)
      }
      if (name === "PBKDF2" || name === "HKDF") return new CryptoKey(material, { name }, false, usages)
      if (name.startsWith("AES-")) return new CryptoKey(material, { name, length: material.length * 8 }, extractable, usages)
      throw new DOMException(`Unrecognized algorithm name: ${name}`, "NotSupportedError")
    },
    async exportKey(format, key) {
      requireKey(key)
      if (!key.extractable) throw new DOMException("key is not extractable", "InvalidAccessError")
      const material = key[kKeyMaterial]
      if (format === "raw") return toArrayBuffer(material)
      if (format === "jwk") {
        const jwk = { kty: "oct", k: base64urlEncode(material), ext: true, key_ops: [...key.usages] }
        if (key.algorithm.name === "HMAC") jwk.alg = { "SHA-1": "HS1", "SHA-256": "HS256", "SHA-384": "HS384", "SHA-512": "HS512" }[key.algorithm.hash.name]
        if (key.algorithm.name.startsWith("AES-")) jwk.alg = `A${key.algorithm.length}${key.algorithm.name.slice(4)}`
        return jwk
      }
      throw new DOMException(`Unsupported key format: ${format}`, "NotSupportedError")
    },
    async generateKey(algorithm, extractable, usages) {
      const alg = normalizeAlgorithm(algorithm)
      const name = algName(alg)
      if (name === "HMAC") {
        const hash = normalizeAlgorithm(alg.hash)
        const bits = alg.length ?? hashBlockBits[subtleHashName(hash)]
        const material = native.randomBytes(Math.ceil(bits / 8))
        return new CryptoKey(material, { name: "HMAC", hash: { name: algName(hash) }, length: bits }, extractable, usages)
      }
      if (name.startsWith("AES-")) {
        const bits = alg.length ?? 256
        return new CryptoKey(native.randomBytes(bits / 8), { name, length: bits }, extractable, usages)
      }
      throw new DOMException(`Unsupported algorithm for key generation: ${name}`, "NotSupportedError")
    },
    async sign(algorithm, key, data) {
      requireKey(key)
      requireUsage(key, "sign")
      if (algName(algorithm) !== "HMAC" || key.algorithm.name !== "HMAC") throw new DOMException("Only HMAC signatures are supported", "NotSupportedError")
      const out = native.hmac(subtleHashName(key.algorithm.hash), key[kKeyMaterial], subtleBytes(data))
      return toArrayBuffer(out)
    },
    async verify(algorithm, key, signature, data) {
      requireKey(key)
      requireUsage(key, "verify")
      if (algName(algorithm) !== "HMAC" || key.algorithm.name !== "HMAC") throw new DOMException("Only HMAC signatures are supported", "NotSupportedError")
      const expected = native.hmac(subtleHashName(key.algorithm.hash), key[kKeyMaterial], subtleBytes(data))
      const given = subtleBytes(signature, "signature")
      if (expected.length !== given.length) return false
      let diff = 0
      for (let i = 0; i < expected.length; i++) diff |= expected[i] ^ given[i]
      return diff === 0
    },
    async deriveBits(algorithm, baseKey, length) {
      requireKey(baseKey, "baseKey")
      requireUsage(baseKey, "deriveBits")
      return toArrayBuffer(deriveRaw(algorithm, baseKey, length))
    },
    async deriveKey(algorithm, baseKey, derivedKeyAlgorithm, extractable, usages) {
      requireKey(baseKey, "baseKey")
      requireUsage(baseKey, "deriveKey")
      const derived = normalizeAlgorithm(derivedKeyAlgorithm)
      const name = algName(derived)
      let bits = derived.length
      if (!bits && name === "HMAC") bits = hashBlockBits[subtleHashName(normalizeAlgorithm(derived.hash))]
      if (!bits) throw new DOMException("derivedKeyAlgorithm.length is required", "OperationError")
      return subtleCrypto.importKey("raw", deriveRaw(algorithm, baseKey, bits), derived, extractable, usages)
    },
    async digest(algorithm, data) {
      return toArrayBuffer(native.digest(subtleHashName(normalizeAlgorithm(algorithm).name), subtleBytes(data)))
    },
    async encrypt() {
      throw new DOMException("encrypt is not supported in this runtime", "NotSupportedError")
    },
    async decrypt() {
      throw new DOMException("decrypt is not supported in this runtime", "NotSupportedError")
    },
    async wrapKey() {
      throw new DOMException("wrapKey is not supported in this runtime", "NotSupportedError")
    },
    async unwrapKey() {
      throw new DOMException("unwrapKey is not supported in this runtime", "NotSupportedError")
    },
  }
  Object.defineProperty(subtleCrypto, Symbol.toStringTag, { value: "SubtleCrypto", configurable: true })

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
  const joinChunks = (chunks) => {
    let total = 0
    for (const c of chunks) total += c.byteLength
    const out = Buffer.allocUnsafe(total)
    let pos = 0
    for (const c of chunks) {
      out.set(c instanceof Uint8Array ? c : new Uint8Array(c.buffer, c.byteOffset, c.byteLength), pos)
      pos += c.byteLength
    }
    return out
  }
  const kZlib = Symbol("zlibState")
  const zlibState = (stream) => {
    let state = stream[kZlib]
    if (!state) {
      state = { op: "gzip", options: undefined, chunks: [] }
      Object.defineProperty(stream, kZlib, { value: state, configurable: true })
    }
    return state
  }
  const zlibHandle = (stream) => ({
    pending: null,
    offset: 0,
    close() {},
    reset() {
      this.pending = null
      zlibState(stream).chunks = []
    },
    params() {},
    writeSync(flushFlag, input, inOff, inLen, out, outOff, outLen) {
      const state = zlibState(stream)
      if (input && inLen > 0) state.chunks.push(new Uint8Array(input.buffer, input.byteOffset + (inOff ?? 0), inLen))
      if (this.pending === null) {
        if (state.chunks.length === 0) return [0, outLen]
        const data = joinChunks(state.chunks)
        if (flushFlag !== zlibConstants.Z_FINISH) {
          try {
            this.pending = zlibRun(state.op, data, state.options)
          } catch {
            return [0, outLen]
          }
        } else {
          this.pending = zlibRun(state.op, data, state.options)
        }
        state.chunks = []
        this.offset = 0
      }
      const n = Math.min(outLen, this.pending.length - this.offset)
      if (n > 0) out.set(this.pending.subarray(this.offset, this.offset + n), outOff)
      this.offset += n
      if (this.offset >= this.pending.length) this.pending = null
      return [0, outLen - n]
    },
  })
  function ZlibStream(op, options) {
    Transform.call(this, { highWaterMark: options?.highWaterMark })
    Object.defineProperty(this, kZlib, { value: { op, options, chunks: [] }, configurable: true })
    this.bytesWritten = 0
    this._chunkSize = options?.chunkSize ?? 16384
    this._outBuffer = Buffer.allocUnsafe(this._chunkSize)
    this._outOffset = 0
    this._buffer = this._outBuffer
    this._offset = 0
    this._hadError = false
    this._handle = zlibHandle(this)
  }
  Object.setPrototypeOf(ZlibStream.prototype, Transform.prototype)
  Object.setPrototypeOf(ZlibStream, Transform)
  Object.assign(ZlibStream.prototype, {
    _transform(chunk, encoding, cb) {
      zlibState(this).chunks.push(chunk)
      this.bytesWritten += chunk.length
      cb()
    },
    _flush(cb) {
      const state = zlibState(this)
      try {
        cb(null, zlibRun(state.op, joinChunks(state.chunks), state.options))
      } catch (e) {
        cb(e)
      }
    },
    _processChunk(chunk, flushFlag) {
      const state = zlibState(this)
      if (chunk && chunk.byteLength) {
        state.chunks.push(chunk)
        this.bytesWritten += chunk.byteLength
      }
      if (flushFlag !== zlibConstants.Z_FINISH) return Buffer.alloc(0)
      const input = joinChunks(state.chunks)
      state.chunks = []
      return zlibRun(state.op, input, state.options)
    },
    flush(kind, cb) {
      if (typeof kind === "function") cb = kind
      if (typeof cb === "function") queueMicrotask(cb)
    },
    close(cb) {
      if (typeof cb === "function") this.once("close", cb)
      this.destroy()
    },
    params(level, strategy, cb) {
      if (typeof cb === "function") queueMicrotask(cb)
    },
    reset() {
      zlibState(this).chunks = []
      if (this._handle) this._handle.pending = null
    },
  })
  const zlib = {
    constants: zlibConstants,
    ...zlibConstants,
    crc32: (data, value = 0) => native.crc32(typeof data === "string" ? Buffer.from(data) : cryptoBytes(data, undefined, "data"), value),
  }
  for (const op of ["gzip", "gunzip", "deflate", "inflate", "deflateRaw", "inflateRaw", "unzip"]) {
    const Name = op[0].toUpperCase() + op.slice(1)
    zlib[op + "Sync"] = (data, options) => zlibRun(op, data, options)
    zlib[op] = zlibAsync(op)
    const Ctor = function (options) {
      if (!(this instanceof Ctor)) return new Ctor(options)
      ZlibStream.call(this, op, options)
    }
    Object.defineProperty(Ctor, "name", { value: Name })
    Object.setPrototypeOf(Ctor.prototype, ZlibStream.prototype)
    Object.setPrototypeOf(Ctor, ZlibStream)
    zlib[Name] = Ctor
    zlib["create" + Name] = (options) => new Ctor(options)
  }
  const unsupportedCodec = (name) => {
    const e = new Error(`${name} compression is not supported in this runtime`)
    e.code = "ERR_ZLIB_UNSUPPORTED"
    return e
  }
  for (const [op, label] of [["brotliCompress", "brotli"], ["brotliDecompress", "brotli"], ["zstdCompress", "zstd"], ["zstdDecompress", "zstd"]]) {
    const Name = op[0].toUpperCase() + op.slice(1)
    zlib[op + "Sync"] = () => {
      throw unsupportedCodec(label)
    }
    zlib[op] = (data, options, cb) => {
      if (typeof options === "function") cb = options
      queueMicrotask(() => cb(unsupportedCodec(label)))
    }
    zlib[Name] = class extends Transform {
      constructor(options) {
        super(options)
        queueMicrotask(() => this.destroy(unsupportedCodec(label)))
      }
    }
    Object.defineProperty(zlib[Name], "name", { value: Name })
    zlib["create" + Name] = (options) => new zlib[Name](options)
  }
  Object.assign(zlibConstants, {
    BROTLI_OPERATION_PROCESS: 0, BROTLI_OPERATION_FLUSH: 1, BROTLI_OPERATION_FINISH: 2, BROTLI_OPERATION_EMIT_METADATA: 3,
    BROTLI_PARAM_MODE: 0, BROTLI_MODE_GENERIC: 0, BROTLI_MODE_TEXT: 1, BROTLI_MODE_FONT: 2, BROTLI_DEFAULT_MODE: 0,
    BROTLI_PARAM_QUALITY: 1, BROTLI_MIN_QUALITY: 0, BROTLI_MAX_QUALITY: 11, BROTLI_DEFAULT_QUALITY: 11,
    BROTLI_PARAM_LGWIN: 2, BROTLI_MIN_WINDOW_BITS: 10, BROTLI_MAX_WINDOW_BITS: 24, BROTLI_LARGE_MAX_WINDOW_BITS: 30, BROTLI_DEFAULT_WINDOW: 22,
    BROTLI_PARAM_LGBLOCK: 3, BROTLI_MIN_INPUT_BLOCK_BITS: 16, BROTLI_MAX_INPUT_BLOCK_BITS: 24,
    BROTLI_PARAM_SIZE_HINT: 5, BROTLI_PARAM_LARGE_WINDOW: 6, BROTLI_DECODER_PARAM_LARGE_WINDOW: 1,
    BROTLI_DECODER_RESULT_ERROR: 0, BROTLI_DECODER_RESULT_SUCCESS: 1, BROTLI_DECODER_RESULT_NEEDS_MORE_INPUT: 2, BROTLI_DECODER_RESULT_NEEDS_MORE_OUTPUT: 3,
    ZSTD_e_continue: 0, ZSTD_e_flush: 1, ZSTD_e_end: 2,
    ZSTD_c_compressionLevel: 100, ZSTD_c_windowLog: 101, ZSTD_c_strategy: 107, ZSTD_c_checksumFlag: 201, ZSTD_c_nbWorkers: 400,
    ZSTD_d_windowLogMax: 100, ZSTD_CLEVEL_DEFAULT: 3,
  })
  Object.assign(zlib, zlibConstants)
  zlib.constants = zlibConstants

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
  const kSock = Symbol("socket")
  const attachSocket = (sock, id) => {
    const st = sock[kSock]
    st.id = id
    const handler = (kind, a, b, c, d) => {
      switch (kind) {
        case "connect": {
          st.connected = true
          sock.connecting = false
          sock.pending = false
          sock.localAddress = a
          sock.localPort = b
          sock.remoteAddress = c
          sock.remotePort = d
          sock.remoteFamily = isIPv6(c) ? "IPv6" : "IPv4"
          sock.readyState = "open"
          for (const [chunk, cb] of st.queued) {
            native.netWrite(st.id, chunk)
            cb?.()
          }
          st.queued = []
          if (st.pendingEnd) native.netEnd(st.id)
          touchTimeout(sock)
          sock.emit("connect")
          sock.emit("ready")
          break
        }
        case "data":
          sock.bytesRead += a.byteLength
          touchTimeout(sock)
          sock.push(Buffer.from(a.buffer, a.byteOffset, a.byteLength))
          break
        case "end":
          sock.readyState = sock.writable ? "writeOnly" : "closed"
          sock.push(null)
          if (!sock.allowHalfOpen && !sock.writableEnded) sock.end()
          break
        case "error": {
          const e = new Error(a)
          e.code = b
          e.syscall = a.split(" ")[0]
          if (sock.remoteAddress) {
            e.address = sock.remoteAddress
            e.port = sock.remotePort
          } else {
            e.address = st.host
            e.port = st.port
          }
          st.hadError = true
          sock.connecting = false
          sock.destroy(e)
          break
        }
        case "close":
          st.closed = true
          clearTimeout(st.timer)
          sock.readyState = "closed"
          if (!sock.destroyed) sock.destroy()
          break
      }
    }
    native.netAttach(id, handler)
    return handler
  }
  const touchTimeout = (sock) => {
    const st = sock[kSock]
    clearTimeout(st.timer)
    if (st.timeout > 0) {
      st.timer = setTimeout(() => sock.emit("timeout"), st.timeout)
      if (typeof st.timer?.unref === "function") st.timer.unref()
    }
  }
  class Socket extends Duplex {
    constructor(options = {}) {
      super({ ...options, allowHalfOpen: options.allowHalfOpen ?? false, objectMode: false })
      this[kSock] = { id: null, connected: false, queued: [], pendingEnd: false, closed: false, hadError: false, timeout: 0, timer: null, host: undefined, port: undefined, tls: !!options[kTls], servername: options.servername, rejectUnauthorized: options.rejectUnauthorized, ca: options.ca }
      this.connecting = false
      this.pending = true
      this.readyState = "closed"
      this.bytesRead = 0
      this.bytesWritten = 0
      this.localAddress = undefined
      this.localPort = undefined
      this.remoteAddress = undefined
      this.remotePort = undefined
      this.remoteFamily = undefined
      this.encrypted = !!options[kTls]
      if (options[kSocketId] !== undefined) {
        const st = this[kSock]
        st.connected = true
        this.pending = false
        this.readyState = "open"
        this.localAddress = options.localAddress
        this.localPort = options.localPort
        this.remoteAddress = options.remoteAddress
        this.remotePort = options.remotePort
        this.remoteFamily = isIPv6(options.remoteAddress) ? "IPv6" : "IPv4"
        attachSocket(this, options[kSocketId])
      }
    }
    connect(...args) {
      const cb = typeof args[args.length - 1] === "function" ? args.pop() : null
      let options = args[0]
      if (typeof options !== "object" || options === null) {
        options = { port: args[0], host: typeof args[1] === "string" ? args[1] : undefined }
      }
      const st = this[kSock]
      if (options.path) {
        const e = new Error("IPC sockets are not supported in this runtime")
        e.code = "ENOTSUP"
        queueMicrotask(() => this.destroy(e))
        return this
      }
      const port = Number(options.port)
      const host = options.host ?? "localhost"
      if (!Number.isInteger(port) || port < 0 || port > 65535) {
        throw new RangeError(`The "options.port" property must be >= 0 and < 65536. Received ${options.port}`)
      }
      st.host = host
      st.port = port
      this.connecting = true
      this.readyState = "opening"
      if (cb) this.once("connect", cb)
      const servername = st.servername ?? options.servername ?? host
      const tlsOptions = st.tls ? { rejectUnauthorized: st.rejectUnauthorized ?? options.rejectUnauthorized, ca: st.ca ?? options.ca } : false
      const id = native.netConnect(host, port, tlsOptions, servername, null)
      attachSocket(this, id)
      if (options.timeout) this.setTimeout(options.timeout)
      return this
    }
    _read() {}
    _write(chunk, encoding, cb) {
      const st = this[kSock]
      const bytes = typeof chunk === "string" ? Buffer.from(chunk, encoding) : chunk
      this.bytesWritten += bytes.length
      if (!st.connected) {
        st.queued.push([bytes, cb])
        return
      }
      if (st.closed) return cb(streamError("ERR_STREAM_DESTROYED", "Cannot call write after a stream was destroyed"))
      native.netWrite(st.id, bytes)
      touchTimeout(this)
      cb()
    }
    _final(cb) {
      const st = this[kSock]
      this.readyState = this.readable ? "readOnly" : "closed"
      if (!st.connected) st.pendingEnd = true
      else if (!st.closed) native.netEnd(st.id)
      cb()
    }
    _destroy(err, cb) {
      const st = this[kSock]
      clearTimeout(st.timer)
      if (st.id !== null && !st.closed) native.netDestroy(st.id)
      this.readyState = "closed"
      cb(err)
    }
    setTimeout(ms, cb) {
      const st = this[kSock]
      st.timeout = Number(ms) || 0
      if (typeof cb === "function") {
        if (st.timeout === 0) this.removeListener("timeout", cb)
        else this.once("timeout", cb)
      }
      touchTimeout(this)
      return this
    }
    setNoDelay() {
      return this
    }
    setKeepAlive() {
      return this
    }
    setEncoding(encoding) {
      return super.setEncoding(encoding)
    }
    ref() {
      const st = this[kSock]
      if (st.id !== null) native.netRef(st.id, true)
      return this
    }
    unref() {
      const st = this[kSock]
      if (st.id !== null) native.netRef(st.id, false)
      return this
    }
    address() {
      if (this.localAddress === undefined) return {}
      return { address: this.localAddress, family: isIPv6(this.localAddress) ? "IPv6" : "IPv4", port: this.localPort }
    }
    get bufferSize() {
      return this.writableLength
    }
    get timeout() {
      return this[kSock].timeout
    }
    resetAndDestroy() {
      return this.destroy()
    }
  }
  const kSocketId = Symbol("socketId")
  const kTls = Symbol("tls")
  net.Socket = Socket
  net.Stream = Socket
  Object.defineProperty(net, Symbol.for("jsq.socketFromId"), {
    value: (o) => new Socket({ [kSocketId]: o.id, localAddress: o.localAddress, localPort: o.localPort, remoteAddress: o.remoteAddress, remotePort: o.remotePort }),
    enumerable: false,
    configurable: true,
  })
  net.createConnection = (...args) => {
    const options = typeof args[0] === "object" && args[0] !== null ? args[0] : {}
    return new Socket(options).connect(...args)
  }
  net.connect = net.createConnection
  class NetServer extends EventEmitter {
    constructor(options, listener) {
      super()
      if (typeof options === "function") [options, listener] = [{}, options]
      if (typeof listener === "function") this.on("connection", listener)
      this[kSock] = { id: null, address: null, connections: new Set(), closing: false, options: options ?? {} }
      this.listening = false
      this.maxConnections = undefined
    }
    listen(...args) {
      const cb = typeof args[args.length - 1] === "function" ? args.pop() : null
      let options = args[0]
      if (typeof options !== "object" || options === null) {
        options = { port: args[0], host: typeof args[1] === "string" ? args[1] : undefined }
      }
      const st = this[kSock]
      if (st.id !== null) throw streamError("ERR_SERVER_ALREADY_LISTEN", "Listen method has been called more than once without closing.")
      if (options.path) {
        const e = new Error("IPC servers are not supported in this runtime")
        e.code = "ENOTSUP"
        queueMicrotask(() => this.emit("error", e))
        return this
      }
      const port = options.port === undefined ? 0 : Number(options.port)
      const host = options.host ?? (options.ipv6Only ? "::" : "0.0.0.0")
      if (cb) this.once("listening", cb)
      let result
      try {
        const material = this instanceof TLSServer ? tlsMaterial(st.options) : [null, null]
        result = native.netListen(host, port, (id, localAddress, localPort, remoteAddress, remotePort) => {
          if (this.maxConnections !== undefined && st.connections.size >= this.maxConnections) {
            native.netDestroy(id)
            this.emit("drop", { localAddress, localPort, remoteAddress, remotePort })
            return
          }
          const sock = new Socket({ [kSocketId]: id, localAddress, localPort, remoteAddress, remotePort, allowHalfOpen: st.options.allowHalfOpen })
          sock.server = this
          st.connections.add(sock)
          sock.once("close", () => {
            st.connections.delete(sock)
            if (st.closing && st.connections.size === 0) this.emit("close")
          })
          this.emit("connection", sock)
        }, material[0], material[1], st.options.ALPNProtocols ?? null)
      } catch (e) {
        queueMicrotask(() => this.emit("error", e))
        return this
      }
      st.id = result[0]
      st.address = { address: result[2], family: isIPv6(result[2]) ? "IPv6" : "IPv4", port: result[1] }
      this.listening = true
      queueMicrotask(() => this.emit("listening"))
      return this
    }
    close(cb) {
      const st = this[kSock]
      if (typeof cb === "function") {
        if (st.id === null) queueMicrotask(() => cb(streamError("ERR_SERVER_NOT_RUNNING", "Server is not running.")))
        else this.once("close", () => cb())
      }
      if (st.id !== null) {
        native.netServerClose(st.id)
        st.id = null
        this.listening = false
        st.closing = true
        if (st.connections.size === 0) queueMicrotask(() => this.emit("close"))
      }
      return this
    }
    closeAllConnections() {
      for (const s of this[kSock].connections) s.destroy()
    }
    closeIdleConnections() {}
    address() {
      return this[kSock].address
    }
    getConnections(cb) {
      queueMicrotask(() => cb(null, this[kSock].connections.size))
    }
    ref() {
      const st = this[kSock]
      if (st.id !== null) native.netServerRef(st.id, true)
      return this
    }
    unref() {
      const st = this[kSock]
      if (st.id !== null) native.netServerRef(st.id, false)
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
  net.getDefaultAutoSelectFamily = () => true
  net.setDefaultAutoSelectFamily = () => {}

  class TLSSocket extends Socket {
    constructor(socket, options = {}) {
      super({ ...options, [kTls]: true })
      this.encrypted = true
      this.authorized = true
      this.authorizationError = null
      this.alpnProtocol = false
    }
    getPeerCertificate() {
      return {}
    }
    getCertificate() {
      return {}
    }
    getProtocol() {
      return "TLSv1.3"
    }
    getCipher() {
      return { name: "TLS_AES_128_GCM_SHA256", standardName: "TLS_AES_128_GCM_SHA256", version: "TLSv1.3" }
    }
    getSession() {
      return undefined
    }
    isSessionReused() {
      return false
    }
    exportKeyingMaterial() {
      throw unsupportedKeys()
    }
  }
  const tls = {
    connect(...args) {
      const cb = typeof args[args.length - 1] === "function" ? args.pop() : null
      let options
      if (typeof args[0] === "object" && args[0] !== null) options = { ...args[0] }
      else options = { port: args[0], host: typeof args[1] === "string" ? args[1] : undefined, ...(typeof args[1] === "object" ? args[1] : typeof args[2] === "object" ? args[2] : {}) }
      const sock = new TLSSocket(null, options)
      if (cb) sock.once("secureConnect", cb)
      sock.once("connect", () => sock.emit("secureConnect"))
      sock.connect(options)
      return sock
    },
    createSecureContext: (options = {}) => ({ context: {}, key: options.key, cert: options.cert, ca: options.ca }),
    createServer(options, listener) {
      return new TLSServer(options, listener)
    },
    checkServerIdentity: () => undefined,
    getCiphers: () => ["tls_aes_128_gcm_sha256", "tls_aes_256_gcm_sha384", "tls_chacha20_poly1305_sha256"],
    rootCertificates: [],
    DEFAULT_MIN_VERSION: "TLSv1.2",
    DEFAULT_MAX_VERSION: "TLSv1.3",
    DEFAULT_ECDH_CURVE: "auto",
    DEFAULT_CIPHERS: "TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256:TLS_AES_128_GCM_SHA256",
    TLSSocket,
  }
  class TLSServer extends NetServer {
    constructor(options, listener) {
      if (typeof options === "function") [options, listener] = [{}, options]
      super(options ?? {})
      if (typeof listener === "function") this.on("secureConnection", listener)
      this.on("connection", (sock) => {
        sock.encrypted = true
        sock.authorized = false
        sock.alpnProtocol = false
        Object.setPrototypeOf(sock, TLSSocket.prototype)
        this.emit("secureConnection", sock)
      })
    }
    setSecureContext(options) {
      Object.assign(this[kSock].options, options)
    }
    addContext() {}
  }
  tls.Server = TLSServer
  const tlsMaterial = (options) => {
    const ctx = options?.secureContext ?? {}
    const key = options?.key ?? ctx.key
    const cert = options?.cert ?? ctx.cert
    if (options?.pfx) {
      const e = new Error("PFX/PKCS#12 certificates are not supported in this runtime; pass PEM key and cert")
      e.code = "ERR_FEATURE_UNAVAILABLE_ON_PLATFORM"
      throw e
    }
    if (key == null || cert == null) {
      const e = new Error("A TLS server needs both key and cert")
      e.code = "ERR_MISSING_ARGS"
      throw e
    }
    return [key, cert]
  }

  const kChild = Symbol("child")
  class ChildProcess extends EventEmitter {
    constructor() {
      super()
      this[kChild] = { id: null, stdoutDone: false, stderrDone: false, exited: false, exitArgs: null }
      this.pid = undefined
      this.exitCode = null
      this.signalCode = null
      this.killed = false
      this.connected = false
      this.spawnfile = undefined
      this.spawnargs = []
      this.stdin = null
      this.stdout = null
      this.stderr = null
      this.stdio = [null, null, null]
    }
    kill(signal = "SIGTERM") {
      const st = this[kChild]
      if (st.id === null || st.exited) return false
      this.killed = native.childKill(st.id, signal)
      return this.killed
    }
    ref() {
      const st = this[kChild]
      if (st.id !== null) native.childRef(st.id, true)
      return this
    }
    unref() {
      const st = this[kChild]
      if (st.id !== null) native.childRef(st.id, false)
      return this
    }
    send() {
      const e = new Error("IPC channels are not supported in this runtime")
      e.code = "ERR_IPC_CHANNEL_CLOSED"
      throw e
    }
    disconnect() {}
    [Symbol.dispose]() {
      this.kill()
    }
  }
  const normalizeStdio = (stdio) => {
    if (stdio === undefined) return ["pipe", "pipe", "pipe"]
    if (typeof stdio === "string") return [stdio, stdio, stdio]
    return [0, 1, 2].map((i) => {
      const v = stdio[i]
      if (v === undefined || v === null || v === "pipe" || v === "overlapped") return "pipe"
      if (v === "inherit" || v === process.stdin || v === process.stdout || v === process.stderr || v === i) return "inherit"
      return "ignore"
    })
  }
  function spawn(file, args, options) {
    if (!Array.isArray(args)) [args, options] = [[], args]
    options = options ?? {}
    const child = new ChildProcess()
    const st = child[kChild]
    child.spawnfile = String(file)
    child.spawnargs = [String(file), ...args.map(String)]
    const stdio = normalizeStdio(options.stdio)
    if (stdio[0] === "pipe") {
      child.stdin = new Writable({
        write(chunk, encoding, cb) {
          if (st.id !== null && !st.exited) native.childWrite(st.id, typeof chunk === "string" ? Buffer.from(chunk, encoding) : chunk)
          cb()
        },
        final(cb) {
          if (st.id !== null) native.childEndStdin(st.id)
          cb()
        },
      })
    } else st.stdinDone = true
    if (stdio[1] === "pipe") child.stdout = new Readable({ read() {} })
    else st.stdoutDone = true
    if (stdio[2] === "pipe") child.stderr = new Readable({ read() {} })
    else st.stderrDone = true
    child.stdio = [child.stdin, child.stdout, child.stderr]
    const maybeClose = () => {
      if (st.exited && st.stdoutDone && st.stderrDone && !st.closed) {
        st.closed = true
        child.emit("close", ...st.exitArgs)
      }
    }
    if (child.stdout) child.stdout.once("end", () => { st.stdoutDone = true; maybeClose() })
    if (child.stderr) child.stderr.once("end", () => { st.stderrDone = true; maybeClose() })
    const handler = (kind, a, b) => {
      switch (kind) {
        case "stdout":
          child.stdout?.push(Buffer.from(a.buffer, a.byteOffset, a.byteLength))
          break
        case "stderr":
          child.stderr?.push(Buffer.from(a.buffer, a.byteOffset, a.byteLength))
          break
        case "error": {
          const e = new Error(a)
          child.emit("error", e)
          break
        }
        case "exit":
          st.exited = true
          child.exitCode = a
          child.signalCode = b
          st.exitArgs = [a, b]
          if (child.stdin && !child.stdin.writableEnded) child.stdin.destroy()
          child.emit("exit", a, b)
          child.stdout?.push(null)
          child.stderr?.push(null)
          queueMicrotask(maybeClose)
          break
      }
    }
    try {
      const shell = options.shell === true || typeof options.shell === "string" ? options.shell : false
      const [id, pid] = native.childSpawn(String(file), args.map(String), { cwd: options.cwd, env: options.env, shell, stdio }, handler)
      st.id = id
      child.pid = pid
      queueMicrotask(() => child.emit("spawn"))
    } catch (e) {
      st.exited = true
      st.exitArgs = [-2, null]
      queueMicrotask(() => {
        child.emit("error", e)
        child.stdout?.push(null)
        child.stderr?.push(null)
        maybeClose()
      })
    }
    return child
  }
  const collect = (child, options, cb, command) => {
    const encoding = options.encoding === undefined ? "utf8" : options.encoding
    const maxBuffer = options.maxBuffer ?? 1024 * 1024
    const out = []
    const err = []
    let total = 0
    let killedForSize = false
    const decode = (chunks) => {
      const buf = Buffer.concat(chunks)
      return encoding === "buffer" || encoding === null ? buf : buf.toString(encoding)
    }
    const guard = (chunk) => {
      total += chunk.length
      if (total > maxBuffer && !killedForSize) {
        killedForSize = true
        child.kill()
      }
    }
    child.stdout?.on("data", (c) => { out.push(c); guard(c) })
    child.stderr?.on("data", (c) => { err.push(c); guard(c) })
    let timer = null
    if (options.timeout > 0) timer = setTimeout(() => child.kill(options.killSignal ?? "SIGTERM"), options.timeout)
    let done = false
    child.once("error", (e) => {
      if (done) return
      done = true
      clearTimeout(timer)
      e.cmd = command
      cb(e, decode(out), decode(err))
    })
    child.once("close", (code, signal) => {
      if (done) return
      done = true
      clearTimeout(timer)
      const stdout = decode(out)
      const stderr = decode(err)
      if (code === 0 && !killedForSize) return cb(null, stdout, stderr)
      let e
      if (killedForSize) {
        e = new RangeError("stdout maxBuffer length exceeded")
        e.code = "ERR_CHILD_PROCESS_STDIO_MAXBUFFER"
      } else {
        e = new Error(`Command failed: ${command}\n${typeof stderr === "string" ? stderr : stderr.toString()}`)
        e.code = code
        e.killed = child.killed
        e.signal = signal
        e.cmd = command
      }
      cb(e, stdout, stderr)
    })
    return child
  }
  function exec(command, options, cb) {
    if (typeof options === "function") [options, cb] = [{}, options]
    options = options ?? {}
    const child = spawn(String(command), [], { ...options, shell: options.shell ?? true })
    return collect(child, options, cb ?? (() => {}), String(command))
  }
  function execFile(file, args, options, cb) {
    if (typeof args === "function") [args, options, cb] = [[], {}, args]
    else if (!Array.isArray(args)) [args, options, cb] = [[], args, options]
    if (typeof options === "function") [options, cb] = [{}, options]
    options = options ?? {}
    const child = spawn(String(file), args, options)
    return collect(child, options, cb ?? (() => {}), [file, ...args].join(" "))
  }
  const promisified = (fn) => (...args) =>
    new Promise((resolve, reject) => {
      fn(...args, (err, stdout, stderr) => {
        if (err) {
          err.stdout = stdout
          err.stderr = stderr
          reject(err)
        } else resolve({ stdout, stderr })
      })
    })
  exec[Symbol.for("nodejs.util.promisify.custom")] = promisified(exec)
  execFile[Symbol.for("nodejs.util.promisify.custom")] = promisified(execFile)
  const ipcWire = (emitter, sock, pending, unwrap) => {
    for (const line of pending) sock.write(line)
    pending.length = 0
    let buffered = ""
    sock.on("data", (chunk) => {
      buffered += chunk.toString("utf8")
      let i
      while ((i = buffered.indexOf("\n")) >= 0) {
        const line = buffered.slice(0, i)
        buffered = buffered.slice(i + 1)
        if (!line) continue
        let message
        try {
          message = JSON.parse(line)
        } catch {
          continue
        }
        unwrap(message)
      }
    })
    sock.on("error", () => {})
    sock.on("close", () => {
      if (emitter.connected) {
        emitter.connected = false
        emitter.emit("disconnect")
      }
    })
  }
  const ipcServer = () => {
    const server = net.createServer()
    server.listen(0, "127.0.0.1")
    server.unref()
    return server
  }
  const attachIpc = (child, server, unwrap) => {
    child.connected = true
    let sock = null
    const pending = []
    server.once("connection", (s) => {
      sock = s
      if (child[kChild]?.unrefed) s.unref()
      server.close()
      ipcWire(child, s, pending, unwrap)
    })
    const baseRef = child.ref.bind(child)
    const baseUnref = child.unref.bind(child)
    child.ref = () => {
      child[kChild].unrefed = false
      if (sock) sock.ref()
      return baseRef()
    }
    child.unref = () => {
      child[kChild].unrefed = true
      if (sock) sock.unref()
      return baseUnref()
    }
    child.send = (message, sendHandle, options, cb) => {
      if (typeof sendHandle === "function") cb = sendHandle
      else if (typeof options === "function") cb = options
      if (!child.connected) {
        const e = new Error("Channel closed")
        e.code = "ERR_IPC_CHANNEL_CLOSED"
        if (typeof cb === "function") queueMicrotask(() => cb(e))
        else queueMicrotask(() => child.emit("error", e))
        return false
      }
      const line = JSON.stringify(message === undefined ? null : message) + "\n"
      if (sock) sock.write(line)
      else pending.push(line)
      if (typeof cb === "function") queueMicrotask(() => cb(null))
      return true
    }
    child.disconnect = () => {
      if (!child.connected) return
      child.connected = false
      if (sock) sock.end()
      else server.close()
      queueMicrotask(() => child.emit("disconnect"))
    }
    child.on("exit", () => {
      if (child.connected) {
        child.connected = false
        child.emit("disconnect")
      }
      if (!sock) server.close()
    })
  }
  function fork(modulePath, args, options) {
    if (!Array.isArray(args)) [args, options] = [[], args]
    options = options ?? {}
    const execArgv = options.execArgv ?? []
    const server = ipcServer()
    const env = { ...(options.env ?? process.env), JSQ_IPC: String(server.address().port) }
    let file = String(modulePath)
    if (file.startsWith("file:")) file = fileURLToPath(file)
    const child = spawn(options.execPath ?? process.execPath, [...execArgv, file, ...args.map(String)], { ...options, env, stdio: options.stdio ?? "inherit" })
    attachIpc(child, server, (message) => child.emit("message", message))
    return child
  }
  const childProcessAsync = { spawn, exec, execFile, fork, ChildProcess }
  const childProcessSync = (base) => {
    const wantsText = (options) => options?.encoding !== undefined && options.encoding !== null && options.encoding !== "buffer"
    const output = (text, options) => (text === null || text === undefined ? text : wantsText(options) ? text : Buffer.from(text, "utf8"))
    const execSync = (command, options) => {
      try {
        return output(base.execSync(command, options), options)
      } catch (e) {
        if (e && typeof e === "object" && "stdout" in e) {
          e.stdout = output(e.stdout, options)
          e.stderr = output(e.stderr, options)
          e.output = [null, e.stdout, e.stderr]
          e.pid = 0
          e.signal = null
        }
        throw e
      }
    }
    const spawnSync = (file, args, options) => {
      if (!Array.isArray(args)) [args, options] = [[], args]
      const r = base.spawnSync(file, args, options)
      if (r.error) return r
      const stdout = output(r.stdout, options)
      const stderr = output(r.stderr, options)
      return { pid: 0, output: [null, stdout, stderr], stdout, stderr, status: r.status, signal: null }
    }
    const execFileSync = (file, args, options) => {
      if (!Array.isArray(args)) [args, options] = [[], args]
      const r = spawnSync(file, args, options)
      if (r.error) throw r.error
      if (r.status !== 0) {
        const e = new Error(`Command failed: ${[file, ...args].join(" ")}`)
        Object.assign(e, { status: r.status, signal: null, stdout: r.stdout, stderr: r.stderr, output: r.output, pid: 0 })
        throw e
      }
      return r.stdout
    }
    return { execSync, spawnSync, execFileSync }
  }

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

  let workerSeq = 0
  const workerError = (payload) => {
    const e = new Error(payload?.message ?? "Worker error")
    if (payload?.name) e.name = payload.name
    if (payload?.stack) e.stack = payload.stack
    if (payload?.code) e.code = payload.code
    return e
  }
  class Worker extends EventEmitter {
    #child
    #server
    #exited = false
    constructor(filename, options = {}) {
      super()
      this.threadId = ++workerSeq
      this.resourceLimits = {}
      this.performance = { eventLoopUtilization: () => ({ idle: 0, active: 0, utilization: 0 }) }
      const server = ipcServer()
      this.#server = server
      const baseEnv = options.env === workerThreads.SHARE_ENV || options.env === undefined ? process.env : options.env
      const env = { ...baseEnv, JSQ_IPC: String(server.address().port), JSQ_WORKER: String(this.threadId), JSQ_WORKER_DATA: JSON.stringify(options.workerData === undefined ? null : options.workerData) }
      let file = String(filename)
      if (file.startsWith("file:")) file = fileURLToPath(file)
      const args = options.eval ? ["-e", file] : [file]
      const stdio = ["pipe", options.stdout ? "pipe" : "inherit", options.stderr ? "pipe" : "inherit"]
      const child = spawn(process.execPath, [...(options.execArgv ?? []), ...args, ...(options.argv ?? []).map(String)], { env, stdio })
      this.#child = child
      this.stdin = options.stdin ? child.stdin : null
      this.stdout = child.stdout
      this.stderr = child.stderr
      attachIpc(child, server, (message) => {
        if (message && typeof message === "object" && "__workerMessage" in message) this.emit("message", message.__workerMessage)
        else if (message && typeof message === "object" && "__workerError" in message) this.emit("error", workerError(message.__workerError))
        else if (message && typeof message === "object" && message.__workerOnline) this.emit("online")
      })
      child.on("error", (e) => this.emit("error", e))
      child.on("exit", (code) => {
        this.#exited = true
        this.emit("exit", code ?? 1)
      })
    }
    postMessage(message) {
      if (!this.#exited) this.#child.send({ __workerMessage: message === undefined ? null : message })
    }
    terminate() {
      return new Promise((resolve) => {
        if (this.#exited) return resolve(1)
        this.#child.once("exit", (code) => resolve(code ?? 1))
        this.#child.kill()
      })
    }
    ref() {
      this.#child.ref()
      return this
    }
    unref() {
      this.#child.unref()
      return this
    }
    getHeapSnapshot() {
      try {
        notSupported("worker.getHeapSnapshot")()
      } catch (e) {
        return Promise.reject(e)
      }
    }
  }
  const workerThreads = {
    isMainThread: true,
    threadId: 0,
    parentPort: null,
    workerData: null,
    resourceLimits: {},
    SHARE_ENV: Symbol.for("nodejs.worker_threads.SHARE_ENV"),
    Worker,
    MessageChannel: globalThis.MessageChannel,
    MessagePort: globalThis.MessagePort,
    BroadcastChannel: globalThis.BroadcastChannel,
    markAsUntransferable() {},
    isMarkedAsUntransferable: () => false,
    markAsUncloneable() {},
    moveMessagePortToContext: (port) => port,
    receiveMessageOnPort: () => undefined,
    setEnvironmentData(key, value) {
      environmentData.set(key, value)
    },
    getEnvironmentData: (key) => environmentData.get(key),
  }
  const environmentData = new Map()
  function setupIpc() {
    const port = Number(process.env.JSQ_IPC)
    if (!port) return
    const sock = net.connect({ port, host: "127.0.0.1" })
    let connected = false
    let listening = false
    const queue = []
    sock.on("connect", () => {
      connected = true
      for (const line of queue) sock.write(line)
      queue.length = 0
      if (!listening) sock.unref()
    })
    process.connected = true
    process.channel = { ref: () => sock.ref(), unref: () => sock.unref() }
    process.send = (message, sendHandle, options, cb) => {
      if (typeof sendHandle === "function") cb = sendHandle
      else if (typeof options === "function") cb = options
      if (!process.connected) {
        const e = new Error("Channel closed")
        e.code = "ERR_IPC_CHANNEL_CLOSED"
        if (typeof cb === "function") queueMicrotask(() => cb(e))
        else throw e
        return false
      }
      const line = JSON.stringify(message === undefined ? null : message) + "\n"
      if (connected) sock.write(line)
      else queue.push(line)
      if (typeof cb === "function") queueMicrotask(() => cb(null))
      return true
    }
    process.disconnect = () => {
      if (!process.connected) return
      process.connected = false
      sock.end()
      queueMicrotask(() => process.emit("disconnect"))
    }
    const isWorker = process.env.JSQ_WORKER !== undefined
    const deliver = (message) => {
      if (isWorker) {
        if (message && typeof message === "object" && "__workerMessage" in message) parentPort.emit("message", message.__workerMessage)
        return
      }
      process.emit("message", message)
    }
    ipcWire(process, sock, [], deliver)
    const refOn = (fn) =>
      function (name, listener) {
        if (name === "message") {
          listening = true
          sock.ref()
        }
        return fn.call(this, name, listener)
      }
    process.on = refOn(process.on)
    process.addListener = refOn(process.addListener)
    process.once = refOn(process.once)
    if (!isWorker) return
    const parentPort = new EventEmitter()
    parentPort.postMessage = (message) => process.send({ __workerMessage: message === undefined ? null : message })
    parentPort.close = () => process.disconnect()
    parentPort.ref = () => sock.ref()
    parentPort.unref = () => sock.unref()
    parentPort.start = () => {}
    const portOn = parentPort.on.bind(parentPort)
    parentPort.on = parentPort.addListener = (name, fn) => {
      if (name === "message") {
        listening = true
        sock.ref()
      }
      return portOn(name, fn)
    }
    workerThreads.isMainThread = false
    workerThreads.threadId = Number(process.env.JSQ_WORKER) || 1
    workerThreads.parentPort = parentPort
    try {
      workerThreads.workerData = JSON.parse(process.env.JSQ_WORKER_DATA ?? "null")
    } catch {
      workerThreads.workerData = null
    }
    process.on("uncaughtException", (e) => {
      process.send({ __workerError: { message: e?.message ?? String(e), name: e?.name, stack: e?.stack, code: e?.code } })
      process.exit(1)
    })
    process.send({ __workerOnline: true })
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
  const contextGlobal = (sandbox) =>
    new Proxy(sandbox, {
      get: (target, key) => (key in target ? target[key] : globalThis[key]),
      has: (target, key) => key in target || key in globalThis,
      set: (target, key, value) => {
        target[key] = value
        return true
      },
    })
  const vmRun = (code, sandbox) => {
    if (sandbox === undefined || sandbox === null) return indirectEval(String(code))
    const scope = "globalThis" in sandbox ? {} : { globalThis: contextGlobal(sandbox), global: contextGlobal(sandbox) }
    const fn = new Function("__sandbox__", "__scope__", `with (__sandbox__) { with (__scope__) { return eval(${JSON.stringify(String(code))}) } }`)
    return fn.call(sandbox, sandbox, scope)
  }
  const vmConstants = { USE_MAIN_CONTEXT_DEFAULT_LOADER: Symbol("vm_dynamic_import_main_context_default"), DONT_CONTEXTIFY: Symbol("vm_context_no_contextify") }
  const vmModule = {
    Script: VmScript,
    constants: vmConstants,
    createContext(sandbox = {}) {
      if (sandbox === vmConstants.DONT_CONTEXTIFY) sandbox = {}
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
    httpsClient,
    WebSocket,
    subtleCrypto,
    CryptoKey,
    StringDecoder,
    querystring,
    tty,
    PerformanceObserver,
    perfHooksExtras,
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
    childProcessAsync,
    childProcessSync,
    setupIpc,
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
    let natives: [(&str, u32, crate::object::NativeFn); 56] = [
        ("isProxy", 1, is_proxy),
        ("watchStart", 4, super::netio::watch_start),
        ("watchClose", 1, super::netio::watch_close),
        ("watchRef", 2, super::netio::watch_ref),
        ("fsOpen", 3, fs_open),
        ("fsClose", 1, fs_close),
        ("fsReadFd", 5, fs_read_fd),
        ("fsWriteFd", 5, fs_write_fd),
        ("fsFstat", 1, fs_fstat),
        ("fsFtruncate", 2, fs_ftruncate),
        ("fsFsync", 1, fs_fsync),
        ("netConnect", 5, super::netio::net_connect),
        ("netAttach", 2, super::netio::net_attach),
        ("netWrite", 2, super::netio::net_write),
        ("netEnd", 1, super::netio::net_end),
        ("netDestroy", 1, super::netio::net_destroy),
        ("netRef", 2, super::netio::net_ref),
        ("netListen", 3, super::netio::net_listen),
        ("netServerClose", 1, super::netio::net_server_close),
        ("netServerRef", 2, super::netio::net_server_ref),
        ("childSpawn", 4, super::netio::child_spawn),
        ("childWrite", 2, super::netio::child_write),
        ("childEndStdin", 1, super::netio::child_end_stdin),
        ("childKill", 2, super::netio::child_kill),
        ("childRef", 2, super::netio::child_ref),
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
    let websocket = vm.get_prop(&exports, "WebSocket")?;
    vm.define_global("WebSocket", websocket);
    let crypto_key = vm.get_prop(&exports, "CryptoKey")?;
    vm.define_global("CryptoKey", crypto_key);
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

pub fn setup_ipc(vm: &mut Vm) -> Result<(), Value> {
    let n = load(vm)?;
    let setup = vm.get_prop(&n, "setupIpc")?;
    vm.call_value(&setup, Value::Undefined, &[])?;
    Ok(())
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
            "os" => vm.import_builtin("os")?,
            "child_process" => {
                let cp = vm.import_builtin("child_process")?;
                let n = load(vm)?;
                let extra = vm.get_prop(&n, "childProcessAsync")?;
                let make_sync = vm.get_prop(&n, "childProcessSync")?;
                let sync =
                    vm.call_value(&make_sync, Value::Undefined, std::slice::from_ref(&cp))?;
                let o = vm.new_object();
                for source in [&cp, &extra, &sync] {
                    for (k, v) in vm.own_entries(source) {
                        o.set_own(k, v);
                    }
                }
                o.set_hidden("default", Value::Object(o.clone()));
                Value::Object(o)
            }
            "http" => {
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
            "https" => {
                let h = vm.import_builtin("http")?;
                let n = load(vm)?;
                let client = vm.get_prop(&n, "httpsClient")?;
                let o = vm.new_object();
                for (k, v) in vm.own_entries(&h) {
                    o.set_own(k, v);
                }
                for (k, v) in vm.own_entries(&client) {
                    o.set_own(k, v);
                }
                let web = super::web::load(vm)?;
                let secure = vm.get_prop(&web, "createSecureServer")?;
                o.set_own("createServer", secure);
                o.set_hidden("default", Value::Object(o.clone()));
                Value::Object(o)
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
                let extras = vm.get_prop(&n, "perfHooksExtras")?;
                for (k, v) in vm.own_entries(&extras) {
                    o.set_own(k, v);
                }
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

fn is_proxy(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(matches!(
        arg(args, 0),
        Value::Object(o) if matches!(o.kind, crate::object::ObjKind::Proxy(_))
    )))
}

pub(crate) fn gunzip_members(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut rest = data;
    loop {
        let mut decoder = flate2::bufread::GzDecoder::new(rest);
        decoder.read_to_end(&mut out)?;
        rest = decoder.into_inner();
        if !rest.starts_with(&[0x1f, 0x8b]) {
            return Ok(out);
        }
    }
}

fn zlib_bytes(op: &str, data: &[u8], level: i32) -> std::io::Result<Vec<u8>> {
    use std::io::Write;
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
        "gunzip" => gunzip_members(data),
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

pub(crate) fn native_inspect(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let mut options = crate::inspect::InspectOptions::default();
    let opts = arg(args, 1);
    if opts.is_object() {
        match vm.get_prop(&opts, "depth")? {
            Value::Null => options.depth = usize::MAX,
            Value::Number(n) if n.is_finite() && n >= 0.0 => options.depth = n as usize,
            Value::Number(n) if n.is_infinite() && n > 0.0 => options.depth = usize::MAX,
            _ => {}
        }
        if let Value::Number(n) = vm.get_prop(&opts, "breakLength")? {
            options.break_length = if n.is_finite() && n >= 0.0 {
                n as usize
            } else {
                usize::MAX
            };
        }
        match vm.get_prop(&opts, "compact")? {
            Value::Bool(false) => options.compact = 0,
            Value::Bool(true) => options.compact = 3,
            Value::Number(n) if n.is_finite() && n >= 0.0 => options.compact = n as usize,
            _ => {}
        }
    }
    Ok(crate::inspect::inspect_with(vm, &arg(args, 0), options).into())
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

fn stat_object(vm: &mut Vm, md: &std::fs::Metadata) -> Value {
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
    Value::Object(o)
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
    Ok(stat_object(vm, &md))
}

const O_WRONLY: u32 = 1;
const O_RDWR: u32 = 2;
const O_CREAT: u32 = 0o100;
const O_EXCL: u32 = 0o200;
const O_TRUNC: u32 = 0o1000;
const O_APPEND: u32 = 0o2000;

fn open_flags(vm: &mut Vm, v: &Value) -> Result<std::fs::OpenOptions, Value> {
    let mut opts = std::fs::OpenOptions::new();
    let bits = match v {
        Value::Undefined | Value::Null => None,
        Value::Number(n) => Some(*n as u32),
        other => {
            let text = vm.to_string(other)?;
            match text.replace('s', "").as_str() {
                "r" => {
                    opts.read(true);
                }
                "r+" => {
                    opts.read(true).write(true);
                }
                "w" => {
                    opts.write(true).create(true).truncate(true);
                }
                "wx" | "xw" => {
                    opts.write(true).create_new(true);
                }
                "w+" => {
                    opts.read(true).write(true).create(true).truncate(true);
                }
                "wx+" | "xw+" => {
                    opts.read(true).write(true).create_new(true);
                }
                "a" => {
                    opts.append(true).create(true);
                }
                "ax" | "xa" => {
                    opts.append(true).create_new(true);
                }
                "a+" => {
                    opts.read(true).append(true).create(true);
                }
                "ax+" | "xa+" => {
                    opts.read(true).append(true).create_new(true);
                }
                _ => {
                    let err = vm.type_error(format!("Unknown file open flag: {text}"));
                    if let Value::Object(o) = &err {
                        o.set_own("code", "ERR_INVALID_ARG_VALUE".into());
                    }
                    return Err(err);
                }
            }
            return Ok(opts);
        }
    };
    let bits = bits.unwrap_or(0);
    let access = bits & 3;
    opts.read(access != O_WRONLY);
    opts.write(access == O_WRONLY || access == O_RDWR || bits & O_APPEND != 0);
    if bits & O_APPEND != 0 {
        opts.append(true);
    }
    if bits & O_CREAT != 0 {
        if bits & O_EXCL != 0 {
            opts.create_new(true);
        } else {
            opts.create(true);
        }
    }
    if bits & O_TRUNC != 0 {
        opts.truncate(true);
    }
    Ok(opts)
}

fn fs_open(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_of(vm, args, 0)?;
    let opts = open_flags(vm, &arg(args, 1))?;
    let file = opts
        .open(&p)
        .map_err(|e| fs_error(vm, &e, "open", &p, None))?;
    let fd = vm.next_fd;
    vm.next_fd += 1;
    vm.io_files.insert(fd, file);
    Ok(Value::Number(fd as f64))
}

fn bad_fd(vm: &mut Vm, syscall: &str) -> Value {
    let err = vm.make_error(
        ErrorKind::Error,
        format!("EBADF: bad file descriptor, {syscall}"),
    );
    if let Value::Object(o) = &err {
        o.set_own("errno", Value::Number(-9.0));
        o.set_own("code", "EBADF".into());
        o.set_own("syscall", syscall.into());
    }
    err
}

fn fd_arg(vm: &mut Vm, args: &[Value], syscall: &str) -> Result<u32, Value> {
    match arg(args, 0) {
        Value::Number(n) if n >= 0.0 && n.fract() == 0.0 => Ok(n as u32),
        _ => Err(bad_fd(vm, syscall)),
    }
}

fn fs_close(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let fd = fd_arg(vm, args, "close")?;
    if fd <= 2 {
        return Ok(Value::Undefined);
    }
    match vm.io_files.remove(&fd) {
        Some(_) => Ok(Value::Undefined),
        None => Err(bad_fd(vm, "close")),
    }
}

fn io_slice(vm: &mut Vm, args: &[Value]) -> Result<(Value, usize, usize), Value> {
    let buffer = arg(args, 1);
    let Some(total) = buffer_source_bytes(&buffer).map(|b| b.len()) else {
        return Err(vm.type_error(
            "The \"buffer\" argument must be an instance of Buffer, TypedArray, or DataView",
        ));
    };
    let offset = match arg(args, 2) {
        Value::Number(n) if n >= 0.0 => n as usize,
        _ => 0,
    };
    let length = match arg(args, 3) {
        Value::Number(n) if n >= 0.0 => n as usize,
        _ => total.saturating_sub(offset),
    };
    if offset > total || offset + length > total {
        return Err(vm.range_error(format!(
            "The value of \"length\" is out of range. It must be >= 0 && <= {}. Received {length}",
            total.saturating_sub(offset)
        )));
    }
    Ok((buffer, offset, length))
}

fn write_into_view(target: &Value, offset: usize, data: &[u8]) {
    let Value::Object(o) = target else {
        return;
    };
    let (store, base) = match &o.kind {
        crate::object::ObjKind::Typed(t) => (t.store(), t.offset),
        crate::object::ObjKind::DataView(t) => (t.store(), t.offset),
        crate::object::ObjKind::ArrayBuffer(b) => (&b.bytes, 0),
        _ => return,
    };
    let mut store = store.borrow_mut();
    let start = (base + offset).min(store.len());
    let end = (start + data.len()).min(store.len());
    store[start..end].copy_from_slice(&data[..end - start]);
}

fn position_arg(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) if *n >= 0.0 => Some(*n as u64),
        _ => None,
    }
}

fn read_full(file: &mut std::fs::File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        let n = file.read(&mut buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    Ok(total)
}

fn fs_read_fd(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    use std::io::{Seek, SeekFrom};
    let fd = fd_arg(vm, args, "read")?;
    let (buffer, offset, length) = io_slice(vm, args)?;
    let position = position_arg(&arg(args, 4));
    let mut tmp = vec![0u8; length];
    let n = if fd == 0 {
        vm.flush();
        std::io::stdin()
            .read(&mut tmp)
            .map_err(|e| fs_error(vm, &e, "read", "<stdin>", None))?
    } else {
        let Some(file) = vm.io_files.get_mut(&fd) else {
            return Err(bad_fd(vm, "read"));
        };
        let result = (|| -> std::io::Result<usize> {
            if let Some(pos) = position {
                let saved = file.stream_position()?;
                file.seek(SeekFrom::Start(pos))?;
                let n = read_full(file, &mut tmp)?;
                file.seek(SeekFrom::Start(saved))?;
                Ok(n)
            } else {
                read_full(file, &mut tmp)
            }
        })();
        result.map_err(|e| fs_error(vm, &e, "read", "", None))?
    };
    write_into_view(&buffer, offset, &tmp[..n]);
    Ok(Value::Number(n as f64))
}

fn fs_write_fd(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    use std::io::{Seek, SeekFrom, Write};
    let fd = fd_arg(vm, args, "write")?;
    let (buffer, offset, length) = io_slice(vm, args)?;
    let bytes = buffer_source_bytes(&buffer).unwrap_or_default();
    let data = &bytes[offset..offset + length];
    let position = position_arg(&arg(args, 4));
    if fd == 1 || fd == 2 {
        let text = String::from_utf8_lossy(data).into_owned();
        if fd == 1 {
            vm.write_out(&text);
        } else {
            vm.write_err(&text);
        }
        return Ok(Value::Number(data.len() as f64));
    }
    let Some(file) = vm.io_files.get_mut(&fd) else {
        return Err(bad_fd(vm, "write"));
    };
    let result = (|| -> std::io::Result<usize> {
        if let Some(pos) = position {
            let saved = file.stream_position()?;
            file.seek(SeekFrom::Start(pos))?;
            file.write_all(data)?;
            file.seek(SeekFrom::Start(saved))?;
        } else {
            file.write_all(data)?;
        }
        Ok(data.len())
    })();
    let n = result.map_err(|e| fs_error(vm, &e, "write", "", None))?;
    Ok(Value::Number(n as f64))
}

fn fs_fstat(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let fd = fd_arg(vm, args, "fstat")?;
    if fd <= 2 {
        let o = vm.new_object();
        o.set_own("mode", Value::Number(f64::from(0o20666)));
        o.set_own("size", Value::Number(0.0));
        o.set_own("atimeMs", Value::Number(0.0));
        o.set_own("mtimeMs", Value::Number(0.0));
        o.set_own("ctimeMs", Value::Number(0.0));
        o.set_own("birthtimeMs", Value::Number(0.0));
        o.set_own("kind", "char".into());
        return Ok(Value::Object(o));
    }
    let md = match vm.io_files.get(&fd) {
        Some(file) => file.metadata(),
        None => return Err(bad_fd(vm, "fstat")),
    };
    let md = md.map_err(|e| fs_error(vm, &e, "fstat", "", None))?;
    Ok(stat_object(vm, &md))
}

fn fs_ftruncate(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let fd = fd_arg(vm, args, "ftruncate")?;
    let len = match arg(args, 1) {
        Value::Number(n) if n >= 0.0 => n as u64,
        _ => 0,
    };
    let Some(file) = vm.io_files.get(&fd) else {
        return Err(bad_fd(vm, "ftruncate"));
    };
    file.set_len(len)
        .map_err(|e| fs_error(vm, &e, "ftruncate", "", None))?;
    Ok(Value::Undefined)
}

fn fs_fsync(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let fd = fd_arg(vm, args, "fsync")?;
    if fd <= 2 {
        vm.flush();
        return Ok(Value::Undefined);
    }
    let Some(file) = vm.io_files.get(&fd) else {
        return Err(bad_fd(vm, "fsync"));
    };
    file.sync_all()
        .map_err(|e| fs_error(vm, &e, "fsync", "", None))?;
    Ok(Value::Undefined)
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
