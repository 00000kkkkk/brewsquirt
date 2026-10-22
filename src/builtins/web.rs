use super::{arg, method, native_fn};
use crate::compiler::Mode;
use crate::io::{self, FetchRequest, FetchResponse, HttpReply, HttpRequest, IoEvent};
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use std::time::Duration;

type R = Result<Value, Value>;

pub const LAZY_GLOBALS: [&str; 15] = [
    "fetch",
    "CustomEvent",
    "Blob",
    "File",
    "FormData",
    "Headers",
    "Request",
    "Response",
    "URL",
    "URLSearchParams",
    "AbortController",
    "AbortSignal",
    "Event",
    "EventTarget",
    "DOMException",
];

const WEB_SRC: &str = r##"
(native) => {
  const lower = (name) => String(name).toLowerCase()
  let internal = false
  const encoder = new TextEncoder()
  const decoder = new TextDecoder()

  const DOM_CODES = {
    IndexSizeError: 1,
    NotFoundError: 8,
    NotSupportedError: 9,
    InvalidStateError: 11,
    SyntaxError: 12,
    InvalidAccessError: 15,
    TypeMismatchError: 17,
    NetworkError: 19,
    AbortError: 20,
    TimeoutError: 23,
    DataCloneError: 25,
  }

  class DOMException extends Error {
    constructor(message = "", name = "Error") {
      super(message)
      const n = typeof name === "object" && name !== null ? String(name.name ?? "Error") : String(name)
      Object.defineProperty(this, "name", { value: n, writable: true, configurable: true })
      const lines = String(this.stack ?? "").split("\n")
      lines[0] = `${n}: ${this.message}`
      Object.defineProperty(this, "stack", { value: lines.join("\n"), writable: true, configurable: true })
    }
    get code() {
      return DOM_CODES[this.name] ?? 0
    }
  }

  const STOP = Symbol("stop")

  class Event {
    #type
    #stopped = false
    constructor(type, init = {}) {
      if (arguments.length === 0) throw new TypeError('The "type" argument must be specified')
      this.#type = String(type)
      this.bubbles = !!init?.bubbles
      this.cancelable = !!init?.cancelable
      this.defaultPrevented = false
      this.timeStamp = performance.now()
      this.target = null
      this.currentTarget = null
    }
    get type() {
      return this.#type
    }
    preventDefault() {
      if (this.cancelable) this.defaultPrevented = true
    }
    stopPropagation() {}
    stopImmediatePropagation() {
      this.#stopped = true
    }
    get [STOP]() {
      return this.#stopped
    }
  }

  class CustomEvent extends Event {
    #detail
    constructor(type, init = {}) {
      super(type, init)
      this.#detail = init?.detail ?? null
    }
    get detail() {
      return this.#detail
    }
  }

  class EventTarget {
    #listeners = new Map()
    addEventListener(type, listener, options) {
      if (listener == null) return
      const once = typeof options === "object" && options !== null && !!options.once
      const signal = typeof options === "object" && options !== null ? options.signal : undefined
      if (signal?.aborted) return
      const key = String(type)
      let list = this.#listeners.get(key)
      if (!list) this.#listeners.set(key, (list = []))
      if (list.some((l) => l.listener === listener)) return
      list.push({ listener, once })
      signal?.addEventListener("abort", () => this.removeEventListener(key, listener), { once: true })
    }
    removeEventListener(type, listener) {
      const list = this.#listeners.get(String(type))
      if (!list) return
      const i = list.findIndex((l) => l.listener === listener)
      if (i >= 0) list.splice(i, 1)
    }
    dispatchEvent(event) {
      if (!(event instanceof Event)) throw new TypeError('The "event" argument must be an instance of Event')
      event.target = this
      event.currentTarget = this
      const handler = this["on" + event.type]
      if (typeof handler === "function") handler.call(this, event)
      for (const l of [...(this.#listeners.get(event.type) ?? [])]) {
        if (event[STOP]) break
        if (l.once) this.removeEventListener(event.type, l.listener)
        if (typeof l.listener === "function") l.listener.call(this, event)
        else l.listener.handleEvent(event)
      }
      event.currentTarget = null
      return !event.defaultPrevented
    }
  }

  const SIGNAL_KEY = Symbol("signal")
  const ABORT = Symbol("abort")

  class AbortSignal extends EventTarget {
    #aborted = false
    #reason = undefined
    #onabort = null
    constructor(key) {
      if (key !== SIGNAL_KEY) throw new TypeError("Illegal constructor")
      super()
    }
    get aborted() {
      return this.#aborted
    }
    get reason() {
      return this.#reason
    }
    get onabort() {
      return this.#onabort
    }
    set onabort(fn) {
      this.#onabort = typeof fn === "function" ? fn : null
    }
    throwIfAborted() {
      if (this.#aborted) throw this.#reason
    }
    [ABORT](reason) {
      if (this.#aborted) return
      this.#aborted = true
      this.#reason = reason === undefined ? new DOMException("This operation was aborted", "AbortError") : reason
      this.dispatchEvent(new Event("abort"))
    }
    static abort(reason) {
      const s = new AbortSignal(SIGNAL_KEY)
      s[ABORT](reason)
      return s
    }
    static timeout(ms) {
      const s = new AbortSignal(SIGNAL_KEY)
      const id = setTimeout(() => {
        s[ABORT](new DOMException("The operation was aborted due to timeout", "TimeoutError"))
      }, Number(ms))
      native.unref(id)
      return s
    }
    static any(signals) {
      const s = new AbortSignal(SIGNAL_KEY)
      const list = [...signals]
      const hit = list.find((x) => x.aborted)
      if (hit) {
        s[ABORT](hit.reason)
        return s
      }
      for (const x of list) x.addEventListener("abort", () => s[ABORT](x.reason), { once: true })
      return s
    }
  }

  class AbortController {
    #signal = new AbortSignal(SIGNAL_KEY)
    get signal() {
      return this.#signal
    }
    abort(reason) {
      this.#signal[ABORT](reason)
    }
  }

  class Headers {
    #map = new Map()
    constructor(init) {
      if (init == null) return
      if (typeof init[Symbol.iterator] === "function") {
        for (const pair of init) {
          const [k, v] = [...pair]
          this.append(k, v)
        }
      } else {
        for (const k of Object.keys(init)) this.append(k, init[k])
      }
    }
    append(name, value) {
      const k = lower(name)
      const v = String(value).trim()
      const cur = this.#map.get(k)
      this.#map.set(k, cur === undefined ? v : k === "set-cookie" ? `${cur}\n${v}` : `${cur}, ${v}`)
    }
    set(name, value) {
      this.#map.set(lower(name), String(value).trim())
    }
    get(name) {
      const v = this.#map.get(lower(name))
      if (v === undefined) return null
      return lower(name) === "set-cookie" ? v.split("\n").join(", ") : v
    }
    getSetCookie() {
      const v = this.#map.get("set-cookie")
      return v === undefined ? [] : v.split("\n")
    }
    has(name) {
      return this.#map.has(lower(name))
    }
    delete(name) {
      this.#map.delete(lower(name))
    }
    forEach(fn, thisArg) {
      for (const [k, v] of this) fn.call(thisArg, v, k, this)
    }
    entries() {
      const out = []
      for (const [k, v] of this.#map) {
        if (k === "set-cookie") for (const c of v.split("\n")) out.push([k, c])
        else out.push([k, v])
      }
      out.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0))
      return out[Symbol.iterator]()
    }
    keys() {
      return [...this.entries()].map((e) => e[0])[Symbol.iterator]()
    }
    values() {
      return [...this.entries()].map((e) => e[1])[Symbol.iterator]()
    }
    [Symbol.iterator]() {
      return this.entries()
    }
    get size() {
      return this.#map.size
    }
    toJSON() {
      return Object.fromEntries(this.entries())
    }
  }

  const PARSE = Symbol("parse")
  const SYNC = Symbol("sync")
  const owners = new WeakMap()

  function formEncode(s) {
    return encodeURIComponent(s)
      .replace(/%20/g, "+")
      .replace(/[!'()~]/g, (c) => "%" + c.charCodeAt(0).toString(16).toUpperCase())
  }

  function formDecode(s) {
    const t = s.replace(/\+/g, " ")
    try {
      return decodeURIComponent(t)
    } catch {
      return t
    }
  }

  class URLSearchParams {
    #list = []
    constructor(init = "") {
      if (init instanceof URLSearchParams) {
        this.#list = [...init]
      } else if (typeof init === "object" && init !== null) {
        if (typeof init[Symbol.iterator] === "function") {
          for (const pair of init) {
            const p = [...pair]
            if (p.length !== 2) throw new TypeError("Each query pair must be an iterable [name, value] tuple")
            this.#list.push([String(p[0]), String(p[1])])
          }
        } else {
          for (const k of Object.keys(init)) this.#list.push([k, String(init[k])])
        }
      } else {
        this[PARSE](String(init))
      }
    }
    [PARSE](s) {
      this.#list = []
      if (s.startsWith("?")) s = s.slice(1)
      for (const part of s.split("&")) {
        if (part === "") continue
        const i = part.indexOf("=")
        const k = i < 0 ? part : part.slice(0, i)
        const v = i < 0 ? "" : part.slice(i + 1)
        this.#list.push([formDecode(k), formDecode(v)])
      }
    }
    #changed() {
      const url = owners.get(this)
      if (url) url[SYNC](this.toString())
    }
    get size() {
      return this.#list.length
    }
    append(name, value) {
      this.#list.push([String(name), String(value)])
      this.#changed()
    }
    delete(name, value) {
      const n = String(name)
      this.#list = this.#list.filter(([k, v]) => k !== n || (value !== undefined && v !== String(value)))
      this.#changed()
    }
    get(name) {
      const n = String(name)
      const hit = this.#list.find(([k]) => k === n)
      return hit ? hit[1] : null
    }
    getAll(name) {
      const n = String(name)
      return this.#list.filter(([k]) => k === n).map(([, v]) => v)
    }
    has(name, value) {
      const n = String(name)
      return this.#list.some(([k, v]) => k === n && (value === undefined || v === String(value)))
    }
    set(name, value) {
      const n = String(name)
      const v = String(value)
      const i = this.#list.findIndex(([k]) => k === n)
      if (i < 0) this.#list.push([n, v])
      else {
        this.#list[i] = [n, v]
        this.#list = this.#list.filter(([k], j) => k !== n || j === i)
      }
      this.#changed()
    }
    sort() {
      this.#list = this.#list
        .map((e, i) => [e, i])
        .sort((a, b) => (a[0][0] < b[0][0] ? -1 : a[0][0] > b[0][0] ? 1 : a[1] - b[1]))
        .map(([e]) => e)
      this.#changed()
    }
    forEach(fn, thisArg) {
      for (const [k, v] of this.#list) fn.call(thisArg, v, k, this)
    }
    entries() {
      return this.#list.map(([k, v]) => [k, v])[Symbol.iterator]()
    }
    keys() {
      return this.#list.map(([k]) => k)[Symbol.iterator]()
    }
    values() {
      return this.#list.map(([, v]) => v)[Symbol.iterator]()
    }
    [Symbol.iterator]() {
      return this.entries()
    }
    toString() {
      return this.#list.map(([k, v]) => formEncode(k) + "=" + formEncode(v)).join("&")
    }
  }

  const DEFAULT_PORTS = { "http:": "80", "https:": "443", "ws:": "80", "wss:": "443", "ftp:": "21" }
  const URL_RE = /^([a-zA-Z][a-zA-Z0-9+.\-]*):(?:\/\/(?:([^:@\/?#]*)(?::([^@\/?#]*))?@)?(\[[^\]]*\]|[^:\/?#]*)(?::([0-9]*))?)?([^?#]*)(\?[^#]*)?(#.*)?$/
  const SCHEME_RE = /^[a-zA-Z][a-zA-Z0-9+.\-]*:/

  const isSpecial = (protocol) => protocol in DEFAULT_PORTS || protocol === "file:"

  function encodeSet(s, re) {
    return s.replace(re, (c) => encodeURIComponent(c))
  }
  const PATH_SET = /[\x00-\x20"<>`{}]|[^\x00-\x7f]/gu
  const QUERY_SET = /[\x00-\x20"<>]|[^\x00-\x7f]/gu
  const FRAGMENT_SET = /[\x00-\x20"<>`]|[^\x00-\x7f]/gu
  const USER_SET = /[\x00-\x20"<>`{}\/:;=@\[\\\]^|]|[^\x00-\x7f]/gu

  function normalizePath(path, special) {
    if (special) path = path.replace(/\\/g, "/")
    if (path === "") return special ? "/" : ""
    const out = []
    const parts = path.split("/")
    for (let i = 0; i < parts.length; i++) {
      const seg = parts[i]
      const last = i === parts.length - 1
      if (seg === "." || seg.toLowerCase() === "%2e") {
        if (last) out.push("")
      } else if (seg === ".." || /^(\.|%2e)(\.|%2e)$/i.test(seg)) {
        if (out.length > 1) out.pop()
        if (last) out.push("")
      } else {
        out.push(seg)
      }
    }
    let joined = out.join("/")
    if (!joined.startsWith("/") && special) joined = "/" + joined
    return encodeSet(joined, PATH_SET)
  }

  function resolveRef(input, base) {
    if (SCHEME_RE.test(input)) return input
    const b = new URL(base)
    const origin = `${b.protocol}//${b.host}`
    if (input.startsWith("//")) return b.protocol + input
    if (input.startsWith("/")) return origin + input
    if (input.startsWith("?")) return origin + b.pathname + input
    if (input.startsWith("#")) return origin + b.pathname + b.search + input
    if (input === "") return origin + b.pathname + b.search
    const dir = b.pathname.slice(0, b.pathname.lastIndexOf("/") + 1)
    return origin + dir + input
  }

  class URL {
    #protocol = ""
    #username = ""
    #password = ""
    #hostname = ""
    #port = ""
    #pathname = ""
    #search = ""
    #hash = ""
    #params = null
    constructor(input, base) {
      let s = String(input).trim()
      if (base !== undefined) s = resolveRef(s, String(base))
      this.#parse(s, String(input))
    }
    #parse(s, original) {
      const m = URL_RE.exec(s)
      if (!m) throw new TypeError(`Invalid URL: ${original}`)
      const protocol = m[1].toLowerCase() + ":"
      const special = isSpecial(protocol)
      if (special && protocol !== "file:" && !m[4]) throw new TypeError(`Invalid URL: ${original}`)
      this.#protocol = protocol
      this.#username = m[2] ? encodeSet(m[2], USER_SET) : ""
      this.#password = m[3] ? encodeSet(m[3], USER_SET) : ""
      this.#hostname = (m[4] ?? "").toLowerCase()
      let port = m[5] ?? ""
      if (port !== "") {
        const n = Number(port)
        if (n > 65535) throw new TypeError(`Invalid URL: ${original}`)
        port = String(n)
      }
      this.#port = port === DEFAULT_PORTS[protocol] ? "" : port
      this.#pathname = m[4] === undefined && !special ? encodeSet(m[6], PATH_SET) : normalizePath(m[6], special)
      this.#search = m[7] && m[7] !== "?" ? encodeSet(m[7], QUERY_SET) : ""
      this.#hash = m[8] && m[8] !== "#" ? encodeSet(m[8], FRAGMENT_SET) : ""
      if (this.#params) this.#params[PARSE](this.#search)
    }
    [SYNC](query) {
      this.#search = query === "" ? "" : "?" + query
    }
    get href() {
      const auth = this.#username || this.#password
        ? this.#username + (this.#password ? ":" + this.#password : "") + "@"
        : ""
      const authority = this.#hostname !== "" || isSpecial(this.#protocol) ? "//" + auth + this.host : ""
      return this.#protocol + authority + this.#pathname + this.#search + this.#hash
    }
    set href(v) {
      this.#parse(String(v), String(v))
    }
    get origin() {
      return this.#protocol in DEFAULT_PORTS ? `${this.#protocol}//${this.host}` : "null"
    }
    get protocol() {
      return this.#protocol
    }
    set protocol(v) {
      const p = String(v).replace(/:.*$/, "").toLowerCase()
      if (/^[a-z][a-z0-9+.\-]*$/.test(p) && isSpecial(p + ":") === isSpecial(this.#protocol)) {
        this.#protocol = p + ":"
        if (this.#port === DEFAULT_PORTS[this.#protocol]) this.#port = ""
      }
    }
    get username() {
      return this.#username
    }
    set username(v) {
      this.#username = encodeSet(String(v), USER_SET)
    }
    get password() {
      return this.#password
    }
    set password(v) {
      this.#password = encodeSet(String(v), USER_SET)
    }
    get host() {
      return this.#port === "" ? this.#hostname : `${this.#hostname}:${this.#port}`
    }
    set host(v) {
      const m = /^(\[[^\]]*\]|[^:\/?#]*)(?::([0-9]*))?/.exec(String(v))
      if (!m || m[1] === "") return
      this.#hostname = m[1].toLowerCase()
      if (m[2] !== undefined) this.port = m[2]
    }
    get hostname() {
      return this.#hostname
    }
    set hostname(v) {
      const h = String(v)
      if (h !== "" && !/[:\/?#]/.test(h)) this.#hostname = h.toLowerCase()
    }
    get port() {
      return this.#port
    }
    set port(v) {
      const s = String(v)
      if (s === "") {
        this.#port = ""
        return
      }
      const m = /^[0-9]+/.exec(s)
      if (!m || Number(m[0]) > 65535) return
      const p = String(Number(m[0]))
      this.#port = p === DEFAULT_PORTS[this.#protocol] ? "" : p
    }
    get pathname() {
      return this.#pathname
    }
    set pathname(v) {
      let p = String(v)
      if (isSpecial(this.#protocol) && !p.startsWith("/")) p = "/" + p
      this.#pathname = normalizePath(p, isSpecial(this.#protocol))
    }
    get search() {
      return this.#search
    }
    set search(v) {
      let s = String(v)
      if (s.startsWith("?")) s = s.slice(1)
      this.#search = s === "" ? "" : "?" + encodeSet(s, QUERY_SET)
      if (this.#params) this.#params[PARSE](this.#search)
    }
    get searchParams() {
      if (!this.#params) {
        this.#params = new URLSearchParams(this.#search)
        owners.set(this.#params, this)
      }
      return this.#params
    }
    get hash() {
      return this.#hash
    }
    set hash(v) {
      let s = String(v)
      if (s.startsWith("#")) s = s.slice(1)
      this.#hash = s === "" ? "" : "#" + encodeSet(s, FRAGMENT_SET)
    }
    toString() {
      return this.href
    }
    toJSON() {
      return this.href
    }
    static canParse(input, base) {
      try {
        new URL(input, base)
        return true
      } catch {
        return false
      }
    }
    static parse(input, base) {
      try {
        return new URL(input, base)
      } catch {
        return null
      }
    }
  }

  const kBytes = Symbol("bytes")
  const concatBytes = (chunks) => {
    const out = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0))
    let at = 0
    for (const c of chunks) {
      out.set(c, at)
      at += c.length
    }
    return out
  }
  const viewBytes = (v) => new Uint8Array(v.buffer.slice(v.byteOffset, v.byteOffset + v.byteLength))

  class Blob {
    #type
    constructor(parts = [], options = {}) {
      if (parts === null || typeof parts !== "object" || typeof parts[Symbol.iterator] !== "function") {
        if (arguments.length > 0) throw new TypeError("Failed to construct 'Blob': The provided value cannot be converted to a sequence.")
        parts = []
      }
      const chunks = []
      for (const p of parts) {
        if (p instanceof Blob) chunks.push(p[kBytes])
        else if (p instanceof ArrayBuffer) chunks.push(new Uint8Array(p.slice(0)))
        else if (ArrayBuffer.isView(p)) chunks.push(viewBytes(p))
        else chunks.push(encoder.encode(options?.endings === "native" ? String(p).replace(/\r?\n/g, EOL) : String(p)))
      }
      this[kBytes] = concatBytes(chunks)
      const t = options?.type === undefined ? "" : String(options.type)
      this.#type = /^[\x20-\x7e]*$/.test(t) ? t.toLowerCase() : ""
    }
    get size() {
      return this[kBytes].length
    }
    get type() {
      return this.#type
    }
    slice(start = 0, end = this.size, contentType = "") {
      const n = this.size
      const s = start < 0 ? Math.max(n + start, 0) : Math.min(start, n)
      const e = end < 0 ? Math.max(n + end, 0) : Math.min(end, n)
      const b = new Blob([], { type: contentType })
      b[kBytes] = this[kBytes].slice(s, Math.max(e, s))
      return b
    }
    async text() {
      return decoder.decode(this[kBytes])
    }
    async arrayBuffer() {
      return this[kBytes].slice().buffer
    }
    async bytes() {
      return this[kBytes].slice()
    }
    stream() {
      throw new TypeError("Blob.prototype.stream is not supported: use arrayBuffer() or text()")
    }
    get [Symbol.toStringTag]() {
      return "Blob"
    }
  }
  const EOL = typeof process !== "undefined" && process.platform === "win32" ? "\r\n" : "\n"

  class File extends Blob {
    #name
    #lastModified
    constructor(parts, name, options = {}) {
      if (arguments.length < 2) throw new TypeError("Failed to construct 'File': 2 arguments required")
      super(parts, options)
      this.#name = String(name).replace(/\//g, ":")
      const lm = options?.lastModified
      this.#lastModified = lm === undefined ? Date.now() : Number(lm) || 0
    }
    get name() {
      return this.#name
    }
    get lastModified() {
      return this.#lastModified
    }
    get webkitRelativePath() {
      return ""
    }
    get [Symbol.toStringTag]() {
      return "File"
    }
  }

  const formBoundaries = new WeakMap()
  class FormData {
    #entries = []
    constructor(form) {
      if (form !== undefined) throw new TypeError("FormData constructor: form elements are not supported")
    }
    #normalize(name, value, filename) {
      name = String(name)
      if (value instanceof Blob) {
        const fname = filename !== undefined ? String(filename) : value instanceof File ? value.name : "blob"
        if (!(value instanceof File) || filename !== undefined) value = new File([value], fname, { type: value.type, lastModified: value instanceof File ? value.lastModified : undefined })
        return [name, value]
      }
      if (filename !== undefined) throw new TypeError("Failed to execute 'append' on 'FormData': parameter 2 is not of type 'Blob'.")
      return [name, String(value)]
    }
    append(name, value, filename) {
      this.#entries.push(this.#normalize(name, value, filename))
    }
    set(name, value, filename) {
      const entry = this.#normalize(name, value, filename)
      const i = this.#entries.findIndex((e) => e[0] === entry[0])
      if (i < 0) this.#entries.push(entry)
      else {
        this.#entries[i] = entry
        this.#entries = this.#entries.filter((e, j) => j <= i || e[0] !== entry[0])
      }
    }
    get(name) {
      name = String(name)
      const e = this.#entries.find((e) => e[0] === name)
      return e ? e[1] : null
    }
    getAll(name) {
      name = String(name)
      return this.#entries.filter((e) => e[0] === name).map((e) => e[1])
    }
    has(name) {
      name = String(name)
      return this.#entries.some((e) => e[0] === name)
    }
    delete(name) {
      name = String(name)
      this.#entries = this.#entries.filter((e) => e[0] !== name)
    }
    *entries() {
      for (const e of this.#entries.slice()) yield [e[0], e[1]]
    }
    *keys() {
      for (const e of this.#entries.slice()) yield e[0]
    }
    *values() {
      for (const e of this.#entries.slice()) yield e[1]
    }
    forEach(fn, thisArg) {
      for (const [k, v] of this.entries()) fn.call(thisArg, v, k, this)
    }
    [Symbol.iterator]() {
      return this.entries()
    }
    get [Symbol.toStringTag]() {
      return "FormData"
    }
  }

  const escapeDisposition = (s) => s.replace(/\n/g, "%0A").replace(/\r/g, "%0D").replace(/"/g, "%22")
  function encodeFormData(form) {
    let boundary = formBoundaries.get(form)
    if (!boundary) {
      boundary = "----JavaSquirtFormBoundary" + Array.from(crypto.getRandomValues(new Uint8Array(12)), (b) => b.toString(16).padStart(2, "0")).join("")
      formBoundaries.set(form, boundary)
    }
    const chunks = []
    for (const [name, value] of form) {
      let head = `--${boundary}\r\nContent-Disposition: form-data; name="${escapeDisposition(name)}"`
      if (value instanceof Blob) {
        head += `; filename="${escapeDisposition(value.name)}"\r\nContent-Type: ${value.type || "application/octet-stream"}`
        chunks.push(encoder.encode(head + "\r\n\r\n"), value[kBytes], encoder.encode("\r\n"))
      } else {
        chunks.push(encoder.encode(head + "\r\n\r\n" + value.replace(/\r(?!\n)|(?<!\r)\n/g, "\r\n") + "\r\n"))
      }
    }
    chunks.push(encoder.encode(`--${boundary}--\r\n`))
    return concatBytes(chunks)
  }

  const indexOfBytes = (hay, needle, from = 0) => {
    outer: for (let i = from; i <= hay.length - needle.length; i++) {
      for (let j = 0; j < needle.length; j++) if (hay[i + j] !== needle[j]) continue outer
      return i
    }
    return -1
  }
  function parseMultipart(bytes, boundary) {
    const form = new FormData()
    const delim = encoder.encode("--" + boundary)
    let pos = indexOfBytes(bytes, delim)
    if (pos < 0) throw new TypeError("Failed to parse multipart body: boundary not found")
    pos += delim.length
    while (pos < bytes.length) {
      if (bytes[pos] === 45 && bytes[pos + 1] === 45) break
      if (bytes[pos] === 13 && bytes[pos + 1] === 10) pos += 2
      const headEnd = indexOfBytes(bytes, encoder.encode("\r\n\r\n"), pos)
      if (headEnd < 0) throw new TypeError("Failed to parse multipart body: malformed part headers")
      const head = decoder.decode(bytes.subarray(pos, headEnd))
      const next = indexOfBytes(bytes, delim, headEnd + 4)
      if (next < 0) throw new TypeError("Failed to parse multipart body: unterminated part")
      let bodyEnd = next
      if (bytes[bodyEnd - 2] === 13 && bytes[bodyEnd - 1] === 10) bodyEnd -= 2
      const body = bytes.slice(headEnd + 4, bodyEnd)
      const name = /name="([^"]*)"/.exec(head)?.[1] ?? ""
      const filename = /filename="([^"]*)"/.exec(head)?.[1]
      const type = /content-type:\s*([^\r\n]+)/i.exec(head)?.[1]?.trim() ?? ""
      if (filename !== undefined) form.append(name, new File([body], filename, { type }))
      else form.append(name, decoder.decode(body))
      pos = next + delim.length
    }
    return form
  }

  function bodyInit(body) {
    if (body == null) return null
    if (typeof body === "string") return body
    if (body instanceof URLSearchParams) return body.toString()
    if (body instanceof Blob) return body[kBytes].slice()
    if (body instanceof FormData) return encodeFormData(body)
    if (body instanceof Uint8Array && internal) return body
    if (body instanceof ArrayBuffer) return new Uint8Array(body.slice(0))
    if (ArrayBuffer.isView(body)) return viewBytes(body)
    return String(body)
  }

  function toBytes(body) {
    if (body == null) return new Uint8Array(0)
    if (typeof body === "string") return encoder.encode(body)
    return body
  }

  function defaultType(body) {
    if (typeof body === "string") return "text/plain;charset=UTF-8"
    if (body instanceof URLSearchParams) return "application/x-www-form-urlencoded;charset=UTF-8"
    if (body instanceof Blob) return body.type || null
    if (body instanceof FormData) return `multipart/form-data; boundary=${formBoundaries.get(body)}`
    return null
  }

  class Body {
    #body
    #used = false
    constructor(body) {
      this.#body = bodyInit(body)
    }
    get body() {
      return this.#body
    }
    get bodyUsed() {
      return this.#used
    }
    #take() {
      if (this.#used) throw new TypeError("Body is unusable: Body has already been read")
      this.#used = true
      return this.#body
    }
    async text() {
      const b = this.#take()
      return typeof b === "string" ? b : b === null ? "" : decoder.decode(b)
    }
    async json() {
      return JSON.parse(await this.text())
    }
    async bytes() {
      return new Uint8Array(await this.arrayBuffer())
    }
    async arrayBuffer() {
      const b = toBytes(this.#take())
      return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength)
    }
    async blob() {
      const b = toBytes(this.#take())
      const type = this.headers?.get("content-type") ?? ""
      return new Blob([b], { type })
    }
    async formData() {
      const type = this.headers?.get("content-type") ?? ""
      const b = toBytes(this.#take())
      const m = /boundary=(?:"([^"]+)"|([^;]+))/i.exec(type)
      if (/multipart\/form-data/i.test(type) && m) return parseMultipart(b, m[1] ?? m[2].trim())
      if (/application\/x-www-form-urlencoded/i.test(type)) {
        const form = new FormData()
        for (const [k, v] of new URLSearchParams(decoder.decode(b))) form.append(k, v)
        return form
      }
      throw new TypeError("Failed to parse body as FormData: unsupported content type")
    }
  }

  class Response extends Body {
    constructor(body = null, init = {}) {
      super(body)
      const status = init.status ?? 200
      if (!internal && (status < 200 || status > 599)) {
        throw new RangeError(`init["status"] must be in the range of 200 to 599, inclusive.`)
      }
      this.status = status
      this.statusText = init.statusText ?? ""
      this.headers = init.headers instanceof Headers && internal ? init.headers : new Headers(init.headers)
      if (!internal && body != null && !this.headers.has("content-type")) {
        const t = defaultType(body)
        if (t) this.headers.set("content-type", t)
      }
      this.ok = status >= 200 && status < 300
      this.redirected = init.redirected ?? false
      this.type = internal ? "basic" : "default"
      this.url = init.url ?? ""
    }
    clone() {
      if (this.bodyUsed) throw new TypeError("Response.clone: Body has already been consumed.")
      return new Response(this.body, this)
    }
    static json(data, init = {}) {
      const headers = new Headers(init.headers)
      if (!headers.has("content-type")) headers.set("content-type", "application/json")
      return new Response(JSON.stringify(data), { ...init, headers })
    }
    static redirect(url, status = 302) {
      if (![301, 302, 303, 307, 308].includes(status)) throw new RangeError("Invalid status code")
      return new Response(null, { status, headers: { location: String(url) } })
    }
  }

  class Request extends Body {
    constructor(input, init = {}) {
      const src = input instanceof Request ? input : null
      const method = String(init.method ?? src?.method ?? "GET").toUpperCase()
      const body = init.body !== undefined ? init.body : src ? src.body : null
      if ((method === "GET" || method === "HEAD") && body != null) {
        throw new TypeError("Request with GET/HEAD method cannot have body.")
      }
      super(body)
      this.method = method
      this.url = src ? src.url : internal ? String(input) : new URL(String(input)).href
      this.headers = new Headers(init.headers ?? src?.headers)
      if (body != null && !this.headers.has("content-type")) {
        const t = defaultType(body)
        if (t) this.headers.set("content-type", t)
      }
      this.redirect = init.redirect ?? src?.redirect ?? "follow"
      this.signal = init.signal ?? src?.signal ?? new AbortSignal(SIGNAL_KEY)
    }
    clone() {
      if (this.bodyUsed) throw new TypeError("Request.clone: Body has already been consumed.")
      return new Request(this)
    }
  }

  async function fetch(input, init = {}) {
    const req = new Request(input, init)
    const url = new URL(req.url)
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      throw new TypeError("fetch failed", { cause: new Error(`unsupported protocol ${url.protocol}`) })
    }
    const signal = req.signal
    signal.throwIfAborted()
    const [id, pending] = native.fetch(
      req.method,
      url.href,
      [...req.headers],
      req.body,
      Number(init.timeout ?? 0),
      req.redirect !== "manual",
    )
    const raw = await new Promise((resolve, reject) => {
      const onAbort = () => {
        native.abort(id)
        reject(signal.reason)
      }
      signal.addEventListener("abort", onAbort, { once: true })
      pending.then(
        (v) => {
          signal.removeEventListener("abort", onAbort)
          resolve(v)
        },
        (e) => {
          signal.removeEventListener("abort", onAbort)
          reject(e)
        },
      )
    })
    if (req.redirect === "error" && raw.status >= 300 && raw.status < 400) {
      throw new TypeError("fetch failed", { cause: new Error("unexpected redirect") })
    }
    internal = true
    try {
      const empty = raw.status === 204 || raw.status === 304 || req.method === "HEAD"
      return new Response(empty ? null : raw.body, {
        status: raw.status,
        statusText: raw.statusText,
        headers: new Headers(raw.headers),
        url: raw.url,
        redirected: raw.url !== url.href,
      })
    } finally {
      internal = false
    }
  }

  function toReply(res) {
    if (res instanceof Response) return [res.status, [...res.headers], res.body ?? ""]
    if (res === undefined || res === null) return [204, [], ""]
    if (typeof res === "string") return [200, [["content-type", "text/plain;charset=UTF-8"]], res]
    return [200, [["content-type", "application/json"]], JSON.stringify(res)]
  }

  function serve(options, handler) {
    if (typeof options === "function") [options, handler] = [{}, options]
    options ??= {}
    handler ??= options.fetch
    if (typeof handler !== "function") throw new TypeError("serve() needs a handler function")
    const onError = options.error
    const [id, port, address] = native.serve(String(options.hostname ?? "127.0.0.1"), Number(options.port ?? 3000), dispatch)
    let open = true
    const server = {
      hostname: address,
      port,
      url: new URL(`http://${address.includes(":") ? `[${address}]` : address}:${port}/`),
      pendingRequests: 0,
      stop() {
        if (open) {
          open = false
          native.close(id)
        }
      },
      close() {
        server.stop()
      },
    }
    function dispatch(raw, replyId) {
      server.pendingRequests++
      internal = true
      let req
      try {
        const hasBody = raw.body.length > 0 && raw.method !== "GET" && raw.method !== "HEAD"
        req = new Request(raw.url, { method: raw.method, headers: raw.headers, body: hasBody ? raw.body : null })
        req.remoteAddress = raw.remote
      } finally {
        internal = false
      }
      Promise.resolve()
        .then(() => handler(req, server))
        .then(toReply, (err) => {
          if (typeof onError === "function") return Promise.resolve().then(() => onError(err)).then(toReply)
          console.error(err)
          return [500, [["content-type", "text/plain;charset=UTF-8"]], "Internal Server Error"]
        })
        .catch((err) => {
          console.error(err)
          return [500, [["content-type", "text/plain;charset=UTF-8"]], "Internal Server Error"]
        })
        .then(([status, headers, body]) => {
          server.pendingRequests--
          native.reply(replyId, status, headers, typeof body === "string" ? body : toBytes(body))
        })
    }
    return server
  }

  class IncomingMessage {
    #bytes
    #encoding = null
    #listeners = {}
    constructor(req) {
      const u = new URL(req.url)
      this.method = req.method
      this.url = u.pathname + u.search
      this.headers = Object.fromEntries(req.headers)
      this.rawHeaders = [...req.headers].flat()
      this.httpVersion = "1.1"
      this.httpVersionMajor = 1
      this.httpVersionMinor = 1
      this.complete = true
      this.aborted = false
      this.socket = { remoteAddress: req.remoteAddress, remotePort: 0, encrypted: false }
      const b = req.body
      this.#bytes = b == null ? new Uint8Array(0) : typeof b === "string" ? encoder.encode(b) : b
    }
    #chunk() {
      if (this.#encoding !== null) return decoder.decode(this.#bytes)
      return typeof Buffer === "function" ? Buffer.from(this.#bytes) : this.#bytes
    }
    setEncoding(encoding) {
      this.#encoding = encoding ?? "utf8"
      return this
    }
    on(event, fn) {
      ;(this.#listeners[event] ??= []).push(fn)
      if (event === "data" && this.#bytes.length !== 0) queueMicrotask(() => fn(this.#chunk()))
      if (event === "end") queueMicrotask(() => queueMicrotask(() => fn()))
      if (event === "close") queueMicrotask(() => queueMicrotask(() => queueMicrotask(() => fn())))
      return this
    }
    once(event, fn) {
      return this.on(event, fn)
    }
    addListener(event, fn) {
      return this.on(event, fn)
    }
    off(event, fn) {
      const list = this.#listeners[event]
      if (list) this.#listeners[event] = list.filter((f) => f !== fn)
      return this
    }
    removeListener(event, fn) {
      return this.off(event, fn)
    }
    removeAllListeners(event) {
      if (event === undefined) this.#listeners = {}
      else delete this.#listeners[event]
      return this
    }
    emit(event, ...args) {
      const list = this.#listeners[event] ?? []
      for (const fn of list) fn(...args)
      return list.length > 0
    }
    pause() {
      return this
    }
    resume() {
      return this
    }
    destroy() {
      return this
    }
    pipe(dest) {
      queueMicrotask(() => {
        if (this.#bytes.length !== 0) dest.write(this.#chunk())
        queueMicrotask(() => dest.end())
      })
      return dest
    }
    text() {
      return Promise.resolve(decoder.decode(this.#bytes))
    }
    json() {
      return this.text().then((t) => JSON.parse(t))
    }
    arrayBuffer() {
      return Promise.resolve(this.#bytes.slice().buffer)
    }
    async *[Symbol.asyncIterator]() {
      if (this.#bytes.length !== 0) yield this.#chunk()
    }
  }

  class ServerResponse {
    #headers = new Headers()
    #chunks = []
    #done
    #listeners = {}
    constructor(done) {
      this.#done = done
      this.statusCode = 200
      this.statusMessage = ""
      this.headersSent = false
      this.writableEnded = false
      this.finished = false
    }
    on(event, fn) {
      ;(this.#listeners[event] ??= []).push(fn)
      return this
    }
    once(event, fn) {
      return this.on(event, fn)
    }
    off(event, fn) {
      const list = this.#listeners[event]
      if (list) this.#listeners[event] = list.filter((f) => f !== fn)
      return this
    }
    removeListener(event, fn) {
      return this.off(event, fn)
    }
    emit(event, ...args) {
      const list = this.#listeners[event] ?? []
      for (const fn of list) fn(...args)
      return list.length > 0
    }
    flushHeaders() {
      this.headersSent = true
    }
    cork() {}
    uncork() {}
    setTimeout(ms, cb) {
      if (typeof cb === "function") this.on("timeout", cb)
      return this
    }
    status(code) {
      this.statusCode = code
      return this
    }
    setHeader(name, value) {
      this.#headers.delete(name)
      for (const v of Array.isArray(value) ? value : [value]) this.#headers.append(name, v)
      return this
    }
    getHeader(name) {
      return this.#headers.get(name) ?? undefined
    }
    getHeaders() {
      return Object.fromEntries(this.#headers)
    }
    hasHeader(name) {
      return this.#headers.has(name)
    }
    removeHeader(name) {
      this.#headers.delete(name)
    }
    writeHead(status, message, headers) {
      if (typeof message === "object" && message !== null) [message, headers] = [undefined, message]
      this.statusCode = status
      if (message !== undefined) this.statusMessage = message
      if (headers) for (const k of Object.keys(headers)) this.setHeader(k, headers[k])
      this.headersSent = true
      return this
    }
    write(chunk) {
      if (chunk != null) this.#chunks.push(chunk)
      this.headersSent = true
      return true
    }
    end(chunk, cb) {
      if (typeof chunk === "function") [chunk, cb] = [undefined, chunk]
      if (this.writableEnded) return this
      if (chunk != null) this.#chunks.push(chunk)
      this.writableEnded = true
      this.finished = true
      this.headersSent = true
      const textual = this.#chunks.every((c) => typeof c !== "object")
      let body
      if (textual) {
        body = this.#chunks.join("")
      } else {
        const parts = this.#chunks.map((c) => (typeof c === "object" ? new Uint8Array(c.buffer ?? c, c.byteOffset ?? 0, c.byteLength) : encoder.encode(String(c))))
        body = new Uint8Array(parts.reduce((n, p) => n + p.length, 0))
        let at = 0
        for (const p of parts) {
          body.set(p, at)
          at += p.length
        }
      }
      internal = true
      try {
        this.#done(new Response(body, { status: this.statusCode, headers: this.#headers }))
      } finally {
        internal = false
      }
      if (typeof cb === "function") queueMicrotask(cb)
      queueMicrotask(() => {
        this.emit("finish")
        this.emit("close")
      })
      return this
    }
  }

  function createServer(options, listener) {
    if (typeof options === "function") [options, listener] = [{}, options]
    let srv = null
    const handlers = { request: listener ? [listener] : [], listening: [], close: [] }
    const emit = (name, ...args) => {
      for (const h of handlers[name] ?? []) h(...args)
    }
    const server = {
      listening: false,
      on(event, fn) {
        ;(handlers[event] ??= []).push(fn)
        return server
      },
      listen(port = 0, host, cb) {
        if (typeof port === "object" && port !== null) [port, host, cb] = [port.port ?? 0, port.host, host]
        if (typeof host === "function") [host, cb] = [undefined, host]
        srv = serve({ port, hostname: host ?? "127.0.0.1" }, (req) =>
          new Promise((resolve) => {
            emit("request", new IncomingMessage(req), new ServerResponse(resolve))
          }),
        )
        server.listening = true
        if (typeof cb === "function") handlers.listening.push(cb)
        queueMicrotask(() => emit("listening"))
        return server
      },
      close(cb) {
        if (srv) srv.stop()
        server.listening = false
        if (typeof cb === "function") handlers.close.push(cb)
        queueMicrotask(() => emit("close"))
        return server
      },
      address() {
        return srv ? { address: srv.hostname, family: srv.hostname.includes(":") ? "IPv6" : "IPv4", port: srv.port } : null
      },
    }
    return server
  }

  const STATUS_CODES = native.statusCodes

  return {
    Headers,
    Request,
    Response,
    URL,
    URLSearchParams,
    AbortController,
    AbortSignal,
    Event,
    CustomEvent,
    EventTarget,
    DOMException,
    Blob,
    File,
    FormData,
    fetch,
    serve,
    createServer,
    STATUS_CODES,
  }
}
"##;

pub fn load(vm: &mut Vm) -> Result<Value, Value> {
    if let Some(v) = &vm.web {
        return Ok(v.clone());
    }
    let dir = std::env::current_dir().unwrap_or_default();
    let proto = match vm.compile(WEB_SRC, "<squirt:web>", dir, Mode::Repl) {
        Ok(p) => p,
        Err(e) => return Err(vm.syntax_error_value(&e, "<squirt:web>")),
    };
    let factory = vm.run_proto(proto)?;
    let natives = vm.new_object();
    method(vm, &natives, "fetch", 6, native_fetch);
    method(vm, &natives, "serve", 3, native_serve);
    method(vm, &natives, "reply", 4, native_reply);
    method(vm, &natives, "close", 1, native_close);
    method(vm, &natives, "abort", 1, native_abort);
    method(vm, &natives, "unref", 1, native_unref);
    let codes = vm.new_object();
    for code in 100..600u16 {
        let text = io::status_text(code);
        if !text.is_empty() {
            codes.set_own(code.to_string(), text.into());
        }
    }
    natives.set_own("statusCodes", Value::Object(codes));
    let exports = vm.call_value(&factory, Value::Undefined, &[Value::Object(natives)])?;
    for name in LAZY_GLOBALS {
        let v = vm.get_prop(&exports, name)?;
        vm.define_global(name, v);
    }
    vm.web = Some(exports.clone());
    Ok(exports)
}

pub fn http_module(vm: &mut Vm) -> Result<Value, Value> {
    let web = load(vm)?;
    let o = vm.new_object();
    for name in [
        "serve",
        "createServer",
        "STATUS_CODES",
        "fetch",
        "Request",
        "Response",
        "Headers",
        "AbortController",
    ] {
        let v = vm.get_prop(&web, name)?;
        o.set_own(name, v);
    }
    o.set_own("default", Value::Object(o.clone()));
    Ok(Value::Object(o))
}

pub fn install(vm: &mut Vm) {
    let f = native_fn(vm, "encodeURIComponent", 1, encode_uri_component);
    vm.define_global("encodeURIComponent", f);
    let f = native_fn(vm, "encodeURI", 1, encode_uri);
    vm.define_global("encodeURI", f);
    let f = native_fn(vm, "decodeURIComponent", 1, decode_uri_component);
    vm.define_global("decodeURIComponent", f);
    let f = native_fn(vm, "decodeURI", 1, decode_uri);
    vm.define_global("decodeURI", f);
}

const URI_RESERVED: &str = ";/?:@&=+$,#";
const URI_MARK: &str = "-_.!~*'()";

fn encode(s: &str, keep: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || URI_MARK.contains(c) || keep.contains(c) {
            out.push(c);
        } else {
            let mut buf = [0; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

fn hex(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|d| d as u8)
}

fn decode(vm: &mut Vm, s: &str, keep: &str) -> R {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let malformed = |vm: &mut Vm| Err(vm.make_error(ErrorKind::URIError, "URI malformed"));
    while i < bytes.len() {
        if bytes[i] != b'%' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let Some(b) = bytes
            .get(i + 1..i + 3)
            .and_then(|h| Some(hex(h[0])? * 16 + hex(h[1])?))
        else {
            return malformed(vm);
        };
        if b < 0x80 {
            if keep.contains(b as char) {
                out.extend_from_slice(&bytes[i..i + 3]);
            } else {
                out.push(b);
            }
            i += 3;
            continue;
        }
        let n = match b {
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            _ => return malformed(vm),
        };
        let mut seq = vec![b];
        let mut j = i + 3;
        for _ in 1..n {
            if bytes.get(j) != Some(&b'%') {
                return malformed(vm);
            }
            let Some(c) = bytes
                .get(j + 1..j + 3)
                .and_then(|h| Some(hex(h[0])? * 16 + hex(h[1])?))
            else {
                return malformed(vm);
            };
            seq.push(c);
            j += 3;
        }
        if std::str::from_utf8(&seq).is_err() {
            return malformed(vm);
        }
        out.extend_from_slice(&seq);
        i = j;
    }
    match String::from_utf8(out) {
        Ok(s) => Ok(s.into()),
        Err(_) => malformed(vm),
    }
}

fn encode_uri_component(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    if !crate::value::is_well_formed(&s) {
        return Err(vm.make_error(ErrorKind::URIError, "URI malformed"));
    }
    Ok(encode(&s, "").into())
}

fn encode_uri(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    if !crate::value::is_well_formed(&s) {
        return Err(vm.make_error(ErrorKind::URIError, "URI malformed"));
    }
    Ok(encode(&s, URI_RESERVED).into())
}

fn decode_uri_component(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    decode(vm, &s, "")
}

fn decode_uri(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    decode(vm, &s, URI_RESERVED)
}

fn pairs(vm: &mut Vm, v: &Value) -> Result<Vec<(String, String)>, Value> {
    let mut out = Vec::new();
    for pair in vm.iterate(v)? {
        let kv = vm.iterate(&pair)?;
        let k = vm.to_string(&kv.first().cloned().unwrap_or_default())?;
        let v = vm.to_string(&kv.get(1).cloned().unwrap_or_default())?;
        out.push((k.to_string(), v.to_string()));
    }
    Ok(out)
}

fn pairs_value(vm: &Vm, headers: Vec<(String, String)>) -> Value {
    let items = headers
        .into_iter()
        .map(|(k, v)| vm.new_array(vec![k.into(), v.into()]))
        .collect();
    vm.new_array(items)
}

fn native_fetch(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let method = vm.to_string(&arg(args, 0))?.to_string();
    let url = vm.to_string(&arg(args, 1))?.to_string();
    let headers = pairs(vm, &arg(args, 2))?;
    let body = match arg(args, 3) {
        Value::Undefined | Value::Null => None,
        v => match super::binary::buffer_source_bytes(&v) {
            Some(b) => Some(b),
            None => Some(vm.to_string(&v)?.as_bytes().to_vec()),
        },
    };
    let ms = vm.to_number(&arg(args, 4))?;
    let timeout = (ms > 0.0).then(|| Duration::from_millis(ms as u64));
    let follow = arg(args, 5).truthy();
    let p = vm.new_promise();
    let id = vm.io.next_id();
    vm.io.pending += 1;
    vm.io_fetches.insert(id, p.clone());
    let pair = vm.new_array(vec![(id as f64).into(), Value::Object(p)]);
    io::spawn_fetch(
        vm.io.tx.clone(),
        id,
        FetchRequest {
            method,
            url,
            headers,
            body,
            timeout,
            follow,
        },
    );
    Ok(pair)
}

fn native_abort(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    if vm.io_fetches.remove(&id).is_some() {
        vm.io.pending -= 1;
    }
    Ok(Value::Undefined)
}

fn native_unref(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    vm.unref_timer(id);
    Ok(Value::Undefined)
}

fn native_serve(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let host = vm.to_string(&arg(args, 0))?.to_string();
    let port = vm.to_number(&arg(args, 1))?;
    if !(0.0..=65535.0).contains(&port) {
        return Err(vm.range_error(format!("port should be >= 0 and < 65536. Received {port}")));
    }
    let dispatch = arg(args, 2);
    let id = vm.io.next_id();
    let handle = match io::listen(&host, port as u16, id, vm.io.tx.clone()) {
        Ok(h) => h,
        Err(e) => {
            return Err(vm.make_error(
                ErrorKind::Error,
                format!("listen failed on {host}:{port}: {e}"),
            ));
        }
    };
    let addr = handle.addr;
    vm.io.pending += 1;
    vm.io_servers.insert(id, (dispatch, handle));
    Ok(vm.new_array(vec![
        (id as f64).into(),
        (addr.port() as f64).into(),
        addr.ip().to_string().into(),
    ]))
}

fn native_reply(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    let status = vm.to_number(&arg(args, 1))? as u16;
    let headers = pairs(vm, &arg(args, 2))?;
    let body = match super::binary::buffer_source_bytes(&arg(args, 3)) {
        Some(b) => b,
        None => vm.to_string(&arg(args, 3))?.as_bytes().to_vec(),
    };
    if let Some(tx) = vm.io_replies.remove(&id) {
        let _ = tx.send(HttpReply {
            status,
            headers,
            body,
        });
    }
    Ok(Value::Undefined)
}

fn native_close(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    if let Some((_, handle)) = vm.io_servers.remove(&id) {
        handle.close();
        vm.io.pending -= 1;
    }
    Ok(Value::Undefined)
}

fn fetch_value(vm: &mut Vm, r: FetchResponse) -> Value {
    let o = vm.new_object();
    o.set_own("status", (r.status as f64).into());
    o.set_own("statusText", r.status_text.into());
    o.set_own("url", r.url.into());
    let headers = pairs_value(vm, r.headers);
    o.set_own("headers", headers);
    o.set_own("body", super::binary::new_uint8(vm, r.body));
    Value::Object(o)
}

fn request_value(vm: &mut Vm, r: HttpRequest) -> Value {
    let o = vm.new_object();
    o.set_own("method", r.method.into());
    o.set_own("url", r.url.into());
    let headers = pairs_value(vm, r.headers);
    o.set_own("headers", headers);
    o.set_own("body", super::binary::new_uint8(vm, r.body));
    o.set_own("remote", r.remote.into());
    Value::Object(o)
}

pub fn handle_io(vm: &mut Vm, ev: IoEvent) -> Result<(), Value> {
    match ev {
        IoEvent::Stdin(chunk) => super::node::handle_stdin(vm, chunk)?,
        IoEvent::Fetch { id, result } => {
            let Some(p) = vm.io_fetches.remove(&id) else {
                return Ok(());
            };
            vm.io.pending -= 1;
            match result {
                Ok(r) => {
                    let v = fetch_value(vm, r);
                    vm.resolve_promise(&p, v);
                }
                Err(msg) => {
                    let e = vm.type_error("fetch failed");
                    let cause = vm.make_error(ErrorKind::Error, msg);
                    if let Value::Object(o) = &e {
                        o.set_own("cause", cause);
                    }
                    vm.reject_promise(&p, e);
                }
            }
        }
        IoEvent::Request {
            server,
            request,
            reply,
        } => {
            let Some((dispatch, _)) = vm.io_servers.get(&server) else {
                return Ok(());
            };
            let dispatch = dispatch.clone();
            let id = vm.io.next_id();
            vm.io_replies.insert(id, reply);
            let raw = request_value(vm, request);
            vm.call_value(&dispatch, Value::Undefined, &[raw, (id as f64).into()])?;
        }
    }
    Ok(())
}
