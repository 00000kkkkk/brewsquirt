use super::{arg, method, native_fn};
use crate::compiler::Mode;
use crate::io::{self, FetchRequest, FetchResponse, HttpReply, HttpRequest, IoEvent, ReplyMsg};
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use std::time::Duration;

type R = Result<Value, Value>;

pub const LAZY_GLOBALS: [&str; 38] = [
    "fetch",
    "MessageEvent",
    "CloseEvent",
    "ErrorEvent",
    "MessageChannel",
    "MessagePort",
    "BroadcastChannel",
    "CustomEvent",
    "Blob",
    "File",
    "FormData",
    "ReadableStream",
    "WritableStream",
    "TransformStream",
    "ReadableStreamDefaultReader",
    "ReadableStreamBYOBReader",
    "ReadableStreamDefaultController",
    "ReadableByteStreamController",
    "ReadableStreamBYOBRequest",
    "WritableStreamDefaultWriter",
    "WritableStreamDefaultController",
    "TransformStreamDefaultController",
    "TextEncoderStream",
    "TextDecoderStream",
    "CompressionStream",
    "DecompressionStream",
    "ByteLengthQueuingStrategy",
    "CountQueuingStrategy",
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

  const DISPATCH = Symbol("dispatch")
  class Event {
    static NONE = 0
    static CAPTURING_PHASE = 1
    static AT_TARGET = 2
    static BUBBLING_PHASE = 3
    #type
    #stopped = false
    #propagationStopped = false
    #bubbles
    #cancelable
    #composed
    #defaultPrevented = false
    #timeStamp
    #target = null
    #currentTarget = null
    #phase = 0
    constructor(type, init = {}) {
      if (arguments.length === 0) throw new TypeError('The "type" argument must be specified')
      this.#type = String(type)
      this.#bubbles = !!init?.bubbles
      this.#cancelable = !!init?.cancelable
      this.#composed = !!init?.composed
      this.#timeStamp = performance.now()
    }
    get type() {
      return this.#type
    }
    get bubbles() {
      return this.#bubbles
    }
    get cancelable() {
      return this.#cancelable
    }
    get composed() {
      return this.#composed
    }
    get defaultPrevented() {
      return this.#cancelable && this.#defaultPrevented
    }
    get timeStamp() {
      return this.#timeStamp
    }
    get target() {
      return this.#target
    }
    get currentTarget() {
      return this.#currentTarget
    }
    get srcElement() {
      return this.#target
    }
    get eventPhase() {
      return this.#phase
    }
    get isTrusted() {
      return false
    }
    get returnValue() {
      return !this.defaultPrevented
    }
    set returnValue(v) {
      if (!v) this.preventDefault()
    }
    get cancelBubble() {
      return this.#propagationStopped
    }
    set cancelBubble(v) {
      if (v) this.stopPropagation()
    }
    composedPath() {
      return this.#currentTarget ? [this.#currentTarget] : []
    }
    initEvent(type, bubbles = false, cancelable = false) {
      this.#type = String(type)
      this.#bubbles = !!bubbles
      this.#cancelable = !!cancelable
    }
    preventDefault() {
      if (this.#cancelable) this.#defaultPrevented = true
    }
    stopPropagation() {
      this.#propagationStopped = true
    }
    stopImmediatePropagation() {
      this.#stopped = true
      this.#propagationStopped = true
    }
    get [STOP]() {
      return this.#stopped
    }
    [DISPATCH](target, current, phase) {
      this.#target = target
      this.#currentTarget = current
      this.#phase = phase
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

  class MessageEvent extends Event {
    #data
    #origin
    #lastEventId
    #source
    #ports
    constructor(type, init = {}) {
      super(type, init)
      this.#data = init?.data ?? null
      this.#origin = init?.origin == null ? "" : String(init.origin)
      this.#lastEventId = init?.lastEventId == null ? "" : String(init.lastEventId)
      this.#source = init?.source ?? null
      this.#ports = Object.freeze([...(init?.ports ?? [])])
    }
    get data() {
      return this.#data
    }
    get origin() {
      return this.#origin
    }
    get lastEventId() {
      return this.#lastEventId
    }
    get source() {
      return this.#source
    }
    get ports() {
      return this.#ports
    }
  }

  class CloseEvent extends Event {
    #wasClean
    #code
    #reason
    constructor(type, init = {}) {
      super(type, init)
      this.#wasClean = !!init?.wasClean
      this.#code = init?.code ?? 0
      this.#reason = init?.reason == null ? "" : String(init.reason)
    }
    get wasClean() {
      return this.#wasClean
    }
    get code() {
      return this.#code
    }
    get reason() {
      return this.#reason
    }
  }

  class ErrorEvent extends Event {
    #message
    #filename
    #lineno
    #colno
    #error
    constructor(type, init = {}) {
      super(type, init)
      this.#message = init?.message == null ? "" : String(init.message)
      this.#filename = init?.filename == null ? "" : String(init.filename)
      this.#lineno = init?.lineno ?? 0
      this.#colno = init?.colno ?? 0
      this.#error = init?.error
    }
    get message() {
      return this.#message
    }
    get filename() {
      return this.#filename
    }
    get lineno() {
      return this.#lineno
    }
    get colno() {
      return this.#colno
    }
    get error() {
      return this.#error
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
      event[DISPATCH](this, this, 2)
      const handler = this["on" + event.type]
      if (typeof handler === "function") handler.call(this, event)
      for (const l of [...(this.#listeners.get(event.type) ?? [])]) {
        if (event[STOP]) break
        if (l.once) this.removeEventListener(event.type, l.listener)
        if (typeof l.listener === "function") l.listener.call(this, event)
        else l.listener.handleEvent(event)
      }
      event[DISPATCH](this, null, 0)
      return !event.defaultPrevented
    }
  }

  const SIGNAL_KEY = Symbol("signal")
  const ABORT = Symbol("abort")

  class NodeEventTarget extends EventTarget {
    #wrapped = []
    #listen(type, fn, once) {
      if (typeof fn !== "function") throw new TypeError(`The "listener" argument must be of type function. Received ${fn === null ? "null" : typeof fn}`)
      const wrapper = (ev) => {
        if (once) this.off(type, fn)
        fn.call(this, ev instanceof MessageEvent ? ev.data : ev)
      }
      this.#wrapped.push({ type, fn, wrapper })
      this.addEventListener(type, wrapper)
      return this
    }
    on(type, fn) {
      return this.#listen(type, fn, false)
    }
    addListener(type, fn) {
      return this.#listen(type, fn, false)
    }
    once(type, fn) {
      return this.#listen(type, fn, true)
    }
    off(type, fn) {
      const i = this.#wrapped.findIndex((w) => w.type === type && w.fn === fn)
      if (i >= 0) {
        this.removeEventListener(type, this.#wrapped[i].wrapper)
        this.#wrapped.splice(i, 1)
      }
      return this
    }
    removeListener(type, fn) {
      return this.off(type, fn)
    }
    removeAllListeners(type) {
      for (const w of this.#wrapped.filter((w) => type === undefined || w.type === type)) this.off(w.type, w.fn)
      return this
    }
    listenerCount(type) {
      return this.#wrapped.filter((w) => w.type === type).length
    }
    listeners(type) {
      return this.#wrapped.filter((w) => w.type === type).map((w) => w.fn)
    }
    eventNames() {
      return [...new Set(this.#wrapped.map((w) => w.type))]
    }
    emit(type, ...args) {
      return this.dispatchEvent(type === "message" ? new MessageEvent(type, { data: args[0] }) : new Event(type))
    }
    setMaxListeners() {
      return this
    }
    getMaxListeners() {
      return 10
    }
  }

  const kPortId = Symbol.for("squirt.portId")
  class MessagePort extends NodeEventTarget {
    #id = 0
    #closed = false
    #onmessage = null
    #refed = true
    constructor(key, id) {
      if (key !== SIGNAL_KEY) throw new TypeError("Illegal constructor")
      super()
      this.#id = id
      this.onmessageerror = null
      this.onclose = null
    }
    get [kPortId]() {
      return this.#id
    }
    get onmessage() {
      return this.#onmessage
    }
    set onmessage(fn) {
      this.#onmessage = typeof fn === "function" ? fn : null
      this.start()
    }
    #listening = 0
    addEventListener(type, listener, options) {
      super.addEventListener(type, listener, options)
      if (String(type) === "message" && typeof listener === "function") {
        this.#listening++
        if (this.#listening === 1 && this.#refed && !this.#closed) native.portRef(this.#id, true)
        this.start()
      }
    }
    removeEventListener(type, listener, options) {
      super.removeEventListener(type, listener, options)
      if (String(type) === "message" && typeof listener === "function" && this.#listening > 0) {
        this.#listening--
        if (this.#listening === 0 && this.#onmessage === null && !this.#closed) native.portRef(this.#id, false)
      }
    }
    postMessage(message, transfer) {
      if (this.#closed) return
      const list = Array.isArray(transfer) ? transfer : transfer && typeof transfer === "object" ? transfer.transfer : undefined
      native.portPost(this.#id, message, list)
    }
    start() {
      if (!this.#closed) native.portStart(this.#id)
    }
    close() {
      if (this.#closed) return
      native.portClose(this.#id)
    }
    ref() {
      this.#refed = true
      if (!this.#closed) native.portRef(this.#id, true)
      return this
    }
    unref() {
      this.#refed = false
      if (!this.#closed) native.portRef(this.#id, false)
      return this
    }
    hasRef() {
      return this.#refed && !this.#closed
    }
    static [Symbol.for("squirt.portClosed")](port) {
      port.#closed = true
    }
  }
  native.portFactory((id) => {
    const port = new MessagePort(SIGNAL_KEY, id)
    const deliver = function (data) {
      this.dispatchEvent(new MessageEvent("message", { data }))
    }
    const closed = function () {
      MessagePort[Symbol.for("squirt.portClosed")](this)
      setTimeout(() => this.dispatchEvent(new Event("close")), 0)
    }
    return [port, deliver, closed]
  })

  class MessageChannel {
    constructor() {
      const [a, b] = native.portPair()
      Object.defineProperty(this, "port1", { value: a, enumerable: true, configurable: true })
      Object.defineProperty(this, "port2", { value: b, enumerable: true, configurable: true })
    }
  }

  class BroadcastChannel extends NodeEventTarget {
    #name
    #id
    #closed = false
    constructor(name) {
      if (arguments.length === 0) throw new TypeError('The "name" argument must be specified')
      super()
      this.onmessage = null
      this.onmessageerror = null
      this.#name = String(name)
      this.#id = native.broadcastOpen(this.#name, (data) => {
        if (!this.#closed) this.dispatchEvent(new MessageEvent("message", { data }))
      })
    }
    get name() {
      return this.#name
    }
    postMessage(message) {
      if (this.#closed) throw new DOMException("BroadcastChannel is closed", "InvalidStateError")
      native.broadcastPost(this.#name, this.#id, message)
    }
    close() {
      if (this.#closed) return
      this.#closed = true
      native.broadcastClose(this.#name, this.#id)
    }
    ref() {
      return this
    }
    unref() {
      return this
    }
  }

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

  const kHeaderList = Symbol("headerList")
  const kWireList = Symbol("wireList")
  class Headers {
    #map = new Map()
    #names = new Map()
    #sorted = null
    constructor(init) {
      if (init == null) return
      if (typeof init === "object" && #map in init) {
        this.#map = new Map(init.#map)
        this.#names = new Map(init.#names)
        return
      }
      if (typeof init[Symbol.iterator] === "function") {
        for (const pair of init) {
          if (Array.isArray(pair) && pair.length === 2) {
            this.append(pair[0], pair[1])
          } else {
            const [k, v] = [...pair]
            this.append(k, v)
          }
        }
      } else {
        for (const k of Object.keys(init)) this.append(k, init[k])
      }
    }
    append(name, value) {
      const k = lower(name)
      const v = String(value).trim()
      const cur = this.#map.get(k)
      this.#sorted = null
      if (cur === undefined) this.#names.set(k, String(name))
      this.#map.set(k, cur === undefined ? v : k === "set-cookie" ? `${cur}\n${v}` : `${cur}, ${v}`)
    }
    set(name, value) {
      this.#sorted = null
      const k = lower(name)
      this.#names.set(k, String(name))
      this.#map.set(k, String(value).trim())
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
      this.#sorted = null
      this.#names.delete(lower(name))
      this.#map.delete(lower(name))
    }
    [kHeaderList]() {
      const out = []
      for (const [k, v] of this.#map) out.push([this.#names.get(k) ?? k, k === "set-cookie" ? v.split("\n").join(", ") : v])
      return out
    }
    [kWireList]() {
      const out = []
      for (const [k, v] of this.#map) {
        const name = this.#names.get(k) ?? k
        if (k === "set-cookie") for (const c of v.split("\n")) out.push([name, c])
        else out.push([name, v])
      }
      return out
    }
    forEach(fn, thisArg) {
      for (const [k, v] of this) fn.call(thisArg, v, k, this)
    }
    entries() {
      if (this.#sorted === null) {
        const out = []
        for (const [k, v] of this.#map) {
          if (k === "set-cookie") for (const c of v.split("\n")) out.push([k, c])
          else out.push([k, v])
        }
        if (out.length > 1) out.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0))
        this.#sorted = out
      }
      const sorted = this.#sorted
      const out = new Array(sorted.length)
      for (let i = 0; i < sorted.length; i++) out[i] = [sorted[i][0], sorted[i][1]]
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
  const invalidUrl = (input) => {
    const e = new TypeError("Invalid URL")
    e.code = "ERR_INVALID_URL"
    e.input = input
    return e
  }
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
      if (!m) throw invalidUrl(original)
      const protocol = m[1].toLowerCase() + ":"
      const special = isSpecial(protocol)
      if (special && protocol !== "file:" && !m[4]) throw invalidUrl(original)
      this.#protocol = protocol
      this.#username = m[2] ? encodeSet(m[2], USER_SET) : ""
      this.#password = m[3] ? encodeSet(m[3], USER_SET) : ""
      this.#hostname = (m[4] ?? "").toLowerCase()
      let port = m[5] ?? ""
      if (port !== "") {
        const n = Number(port)
        if (n > 65535) throw invalidUrl(original)
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
      if (this.#protocol === "blob:") {
        try {
          const inner = new URL(this.pathname)
          return inner.protocol === "http:" || inner.protocol === "https:" ? inner.origin : "null"
        } catch {
          return "null"
        }
      }
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
    static parse(input, base) {
      try {
        return new URL(input, base)
      } catch {
        return null
      }
    }
    static canParse(input, base) {
      try {
        new URL(input, base)
        return true
      } catch {
        return false
      }
    }
  }

  const kStream = Symbol("streamState")
  const kDisturbed = Symbol.for("squirt.stream.disturbed")
  const kErrored = Symbol.for("squirt.stream.errored")
  const defer = () => {
    let resolve
    let reject
    const promise = new Promise((res, rej) => {
      resolve = res
      reject = rej
    })
    promise.catch(() => {})
    return { promise, resolve, reject }
  }
  const chunkSize = (strategy, chunk) => {
    if (typeof strategy?.size === "function") {
      const n = Number(strategy.size(chunk))
      if (!Number.isFinite(n) || n < 0) throw new RangeError("The return value of a queuing strategy's size function must be a finite, non-NaN, non-negative number")
      return n
    }
    return 1
  }
  const highWater = (strategy, fallback) => {
    const hw = strategy?.highWaterMark
    if (hw === undefined) return fallback
    const n = Number(hw)
    if (Number.isNaN(n) || n < 0) throw new RangeError("Invalid highWaterMark")
    return n
  }

  class CountQueuingStrategy {
    #hw
    constructor(init) {
      this.#hw = Number(init?.highWaterMark)
    }
    get highWaterMark() {
      return this.#hw
    }
    get size() {
      return () => 1
    }
  }
  class ByteLengthQueuingStrategy {
    #hw
    constructor(init) {
      this.#hw = Number(init?.highWaterMark)
    }
    get highWaterMark() {
      return this.#hw
    }
    get size() {
      return (chunk) => chunk.byteLength
    }
  }

  class ReadableStreamDefaultController {
    #st
    constructor(st, key) {
      if (key !== kStream) throw new TypeError("Illegal constructor")
      this.#st = st
    }
    get desiredSize() {
      const st = this.#st
      if (st.state === "errored") return null
      if (st.state === "closed") return 0
      return st.highWaterMark - st.queueSize
    }
    enqueue(chunk) {
      const st = this.#st
      if (st.closeRequested || st.state !== "readable") throw new TypeError("The stream is not in a state that permits enqueue")
      if (st.type === "bytes" && !(chunk instanceof Uint8Array)) {
        if (ArrayBuffer.isView(chunk)) chunk = viewBytes(chunk)
        else if (chunk instanceof ArrayBuffer) chunk = new Uint8Array(chunk)
        else throw new TypeError("The provided value is not of type 'ArrayBufferView'")
      }
      const pending = st.reads.shift()
      if (pending) pending.resolve({ value: chunk, done: false })
      else {
        const size = st.type === "bytes" ? chunk.byteLength : chunkSize(st.strategy, chunk)
        st.queue.push({ chunk, size })
        st.queueSize += size
      }
      pullIfNeeded(st)
    }
    close() {
      const st = this.#st
      if (st.closeRequested || st.state !== "readable") throw new TypeError("The stream is not in a state that permits close")
      st.closeRequested = true
      if (st.queue.length === 0) finishClose(st)
    }
    error(e) {
      errorStream(this.#st, e)
    }
  }

  const elementSize = (view) => view.BYTES_PER_ELEMENT ?? 1
  const filledView = (r, done) => {
    const usable = r.filled - (r.filled % r.elementSize)
    const view = r.ctor === DataView ? new DataView(r.buffer, r.byteOffset, usable) : new r.ctor(r.buffer, r.byteOffset, usable / r.elementSize)
    return { value: view, done }
  }
  const settleIntoReads = (st, done) => {
    for (const r of st.byobReads.splice(0)) r.d.resolve(filledView(r, done))
    st.byobRequest = null
  }
  const fillFromQueue = (st, r) => {
    while (st.queue.length && r.filled < r.byteLength) {
      const head = st.queue[0]
      const n = Math.min(head.chunk.byteLength, r.byteLength - r.filled)
      new Uint8Array(r.buffer, r.byteOffset + r.filled, n).set(head.chunk.subarray(0, n))
      r.filled += n
      st.queueSize -= n
      if (n === head.chunk.byteLength) st.queue.shift()
      else st.queue[0] = { chunk: head.chunk.subarray(n), size: head.chunk.byteLength - n }
    }
  }
  const commitRead = (st, r) => {
    const extra = r.filled % r.elementSize
    if (extra) {
      const rest = new Uint8Array(r.buffer, r.byteOffset + r.filled - extra, extra).slice()
      st.queue.unshift({ chunk: rest, size: extra })
      st.queueSize += extra
      r.filled -= extra
    }
    r.d.resolve(filledView(r, false))
  }
  function byteEnqueue(st, bytes) {
    let offset = 0
    while (offset < bytes.length && st.byobReads.length) {
      const r = st.byobReads[0]
      const n = Math.min(r.byteLength - r.filled, bytes.length - offset)
      new Uint8Array(r.buffer, r.byteOffset + r.filled, n).set(bytes.subarray(offset, offset + n))
      r.filled += n
      offset += n
      if (r.filled < r.minBytes) break
      st.byobReads.shift()
      st.byobRequest = null
      commitRead(st, r)
    }
    const rest = offset === 0 ? bytes : bytes.subarray(offset)
    if (!rest.length) return
    const pending = st.reads.shift()
    if (pending) pending.resolve({ value: rest.slice(), done: false })
    else {
      st.queue.push({ chunk: rest.slice(), size: rest.length })
      st.queueSize += rest.length
    }
  }
  function respondInto(st, bytesWritten) {
    const r = st.byobReads[0]
    if (!r) throw new TypeError("There is no pending BYOB read to respond to")
    if (st.state === "closed" || st.closeRequested) {
      if (bytesWritten !== 0) throw new TypeError("bytesWritten must be 0 when calling respond() on a closed stream")
      st.byobReads.shift()
      st.byobRequest = null
      r.d.resolve(filledView(r, true))
      return
    }
    if (bytesWritten <= 0 || r.filled + bytesWritten > r.byteLength) throw new RangeError("bytesWritten out of range")
    r.filled += bytesWritten
    st.byobRequest = null
    if (r.filled >= r.minBytes) {
      st.byobReads.shift()
      commitRead(st, r)
    }
    pullIfNeeded(st)
  }
  class ReadableStreamBYOBRequest {
    #st
    #r
    constructor(st, r, key) {
      if (key !== kStream) throw new TypeError("Illegal constructor")
      this.#st = st
      this.#r = r
    }
    get view() {
      const r = this.#r
      if (this.#st.byobRequest !== this) return null
      return new Uint8Array(r.buffer, r.byteOffset + r.filled, r.byteLength - r.filled)
    }
    respond(bytesWritten) {
      if (this.#st.byobRequest !== this) throw new TypeError("This BYOB request has been invalidated")
      respondInto(this.#st, Number(bytesWritten))
    }
    respondWithNewView(view) {
      if (this.#st.byobRequest !== this) throw new TypeError("This BYOB request has been invalidated")
      if (!ArrayBuffer.isView(view)) throw new TypeError("The provided value is not of type 'ArrayBufferView'")
      const r = this.#r
      if (view.byteOffset !== r.byteOffset + r.filled) throw new RangeError("The region specified by view does not match byobRequest")
      if (view.buffer !== r.buffer) r.buffer = view.buffer
      respondInto(this.#st, view.byteLength)
    }
  }
  class ReadableByteStreamController {
    #st
    constructor(st, key) {
      if (key !== kStream) throw new TypeError("Illegal constructor")
      this.#st = st
    }
    get byobRequest() {
      const st = this.#st
      if (!st.byobReads.length) return null
      if (!st.byobRequest) st.byobRequest = new ReadableStreamBYOBRequest(st, st.byobReads[0], kStream)
      return st.byobRequest
    }
    get desiredSize() {
      const st = this.#st
      if (st.state === "errored") return null
      if (st.state === "closed") return 0
      return st.highWaterMark - st.queueSize
    }
    enqueue(chunk) {
      const st = this.#st
      if (!ArrayBuffer.isView(chunk)) throw new TypeError("The provided value is not of type 'ArrayBufferView'")
      if (chunk.byteLength === 0) throw new TypeError("chunk must have non-zero byteLength")
      if (st.closeRequested || st.state !== "readable") throw new TypeError("The stream is not in a state that permits enqueue")
      byteEnqueue(st, viewBytes(chunk))
      pullIfNeeded(st)
    }
    close() {
      const st = this.#st
      if (st.closeRequested || st.state !== "readable") throw new TypeError("The stream is not in a state that permits close")
      const r = st.byobReads[0]
      if (r && r.filled % r.elementSize) {
        const e = new TypeError("Insufficient bytes to fill elements in the given buffer")
        errorStream(st, e)
        throw e
      }
      st.closeRequested = true
      if (st.queue.length === 0) finishClose(st)
    }
    error(e) {
      errorStream(this.#st, e)
    }
  }
  class ReadableStreamBYOBReader {
    constructor(stream) {
      if (!(stream instanceof ReadableStream)) throw new TypeError("ReadableStreamBYOBReader constructor argument must be a ReadableStream")
      const st = stream[kStream]
      if (st.type !== "bytes") throw new TypeError("Cannot construct a ReadableStreamBYOBReader for a stream not constructed with a byte source")
      if (st.reader) throw new TypeError("ReadableStream is locked")
      this[kStream] = { stream, st, closed: defer() }
      st.reader = this
      if (st.state === "closed") this[kStream].closed.resolve()
      if (st.state === "errored") this[kStream].closed.reject(st.storedError)
    }
    get closed() {
      return this[kStream].closed.promise
    }
    read(view, options) {
      const { st } = this[kStream]
      if (!ArrayBuffer.isView(view)) return Promise.reject(new TypeError("view must be an ArrayBufferView"))
      if (view.byteLength === 0) return Promise.reject(new TypeError("view must have non-zero byteLength"))
      if (view.buffer.byteLength === 0) return Promise.reject(new TypeError("view's buffer must have non-zero byteLength"))
      const size = elementSize(view)
      const length = view.byteLength / size
      const min = options?.min === undefined ? 1 : Number(options.min)
      if (!(min > 0)) return Promise.reject(new TypeError("options.min must be greater than 0"))
      if (min > length) return Promise.reject(new RangeError("options.min must be less than or equal to view's length"))
      if (!st || st.reader !== this) return Promise.reject(new TypeError("This readable stream reader has been released"))
      st.disturbed = true
      if (st.state === "errored") return Promise.reject(st.storedError)
      const { byteOffset, byteLength } = view
      const buffer = typeof view.buffer.transfer === "function" ? view.buffer.transfer() : view.buffer
      const r = { ctor: view.constructor, buffer, byteOffset, byteLength, filled: 0, elementSize: size, minBytes: min * size, d: defer() }
      if (st.state === "closed") {
        r.d.resolve(filledView(r, true))
        return r.d.promise
      }
      fillFromQueue(st, r)
      if (r.filled >= r.minBytes) {
        commitRead(st, r)
        if (st.closeRequested && st.queue.length === 0) finishClose(st)
        else pullIfNeeded(st)
        return r.d.promise
      }
      if (st.closeRequested) {
        if (r.filled % r.elementSize) {
          const e = new TypeError("Insufficient bytes to fill elements in the given buffer")
          errorStream(st, e)
          return Promise.reject(e)
        }
        r.d.resolve(filledView(r, true))
        finishClose(st)
        return r.d.promise
      }
      st.byobReads.push(r)
      pullIfNeeded(st)
      return r.d.promise
    }
    releaseLock() {
      const { st } = this[kStream]
      if (!st || st.reader !== this) return
      const e = new TypeError("Reader was released")
      for (const r of st.byobReads.splice(0)) r.d.reject(e)
      st.byobRequest = null
      if (st.state === "readable") this[kStream].closed.reject(e)
      st.reader = null
      this[kStream].st = null
    }
    cancel(reason) {
      const { st } = this[kStream]
      if (!st || st.reader !== this) return Promise.reject(new TypeError("This readable stream reader has been released"))
      return cancelStream(st, reason)
    }
  }

  function finishClose(st) {
    if (st.state !== "readable") return
    st.state = "closed"
    for (const r of st.reads.splice(0)) r.resolve({ value: undefined, done: true })
    if (st.byobReads) settleIntoReads(st, true)
    if (st.reader) st.reader[kStream].closed.resolve()
  }
  function errorStream(st, e) {
    if (st.state !== "readable") return
    st.state = "errored"
    st.storedError = e
    st.queue = []
    st.queueSize = 0
    for (const r of st.reads.splice(0)) r.reject(e)
    if (st.byobReads) {
      for (const r of st.byobReads.splice(0)) r.d.reject(e)
      st.byobRequest = null
    }
    if (st.reader) st.reader[kStream].closed.reject(e)
  }
  function pullIfNeeded(st) {
    if (!st.started || st.closeRequested || st.state !== "readable") return
    if (st.reads.length === 0 && !st.byobReads?.length && st.highWaterMark - st.queueSize <= 0) return
    if (st.pulling) {
      st.pullAgain = true
      return
    }
    st.pulling = true
    Promise.resolve()
      .then(() => st.source.pull?.(st.controller))
      .then(
        () => {
          st.pulling = false
          if (st.pullAgain) {
            st.pullAgain = false
            pullIfNeeded(st)
          }
        },
        (e) => {
          st.pulling = false
          errorStream(st, e)
        },
      )
  }
  function cancelStream(st, reason) {
    st.disturbed = true
    if (st.state === "closed") return Promise.resolve()
    if (st.state === "errored") return Promise.reject(st.storedError)
    st.queue = []
    st.queueSize = 0
    if (st.byobReads) {
      for (const r of st.byobReads.splice(0)) r.d.resolve({ value: undefined, done: true })
      st.byobRequest = null
    }
    finishClose(st)
    return Promise.resolve()
      .then(() => st.source.cancel?.(reason))
      .then(() => undefined)
  }

  class ReadableStreamDefaultReader {
    constructor(stream) {
      if (!(stream instanceof ReadableStream)) throw new TypeError("ReadableStreamDefaultReader constructor argument must be a ReadableStream")
      const st = stream[kStream]
      if (st.reader) throw new TypeError("ReadableStream is locked")
      this[kStream] = { stream, st, closed: defer() }
      st.reader = this
      if (st.state === "closed") this[kStream].closed.resolve()
      if (st.state === "errored") this[kStream].closed.reject(st.storedError)
    }
    get closed() {
      return this[kStream].closed.promise
    }
    read() {
      const { st } = this[kStream]
      if (!st || st.reader !== this) return Promise.reject(new TypeError("This readable stream reader has been released"))
      st.disturbed = true
      if (st.queue.length) {
        const { chunk, size } = st.queue.shift()
        st.queueSize -= size
        if (st.closeRequested && st.queue.length === 0) finishClose(st)
        else pullIfNeeded(st)
        return Promise.resolve({ value: chunk, done: false })
      }
      if (st.state === "closed") return Promise.resolve({ value: undefined, done: true })
      if (st.state === "errored") return Promise.reject(st.storedError)
      const d = defer()
      st.reads.push(d)
      pullIfNeeded(st)
      return d.promise
    }
    releaseLock() {
      const { st } = this[kStream]
      if (!st || st.reader !== this) return
      const e = new TypeError("Reader was released")
      for (const r of st.reads.splice(0)) r.reject(e)
      if (st.state === "readable") this[kStream].closed.reject(e)
      st.reader = null
      this[kStream].st = null
    }
    cancel(reason) {
      const { st } = this[kStream]
      if (!st || st.reader !== this) return Promise.reject(new TypeError("This readable stream reader has been released"))
      return cancelStream(st, reason)
    }
  }

  class ReadableStream {
    constructor(underlyingSource = {}, strategy = {}) {
      if (underlyingSource === null) underlyingSource = {}
      const type = underlyingSource.type === undefined ? undefined : String(underlyingSource.type)
      if (type !== undefined && type !== "bytes") throw new TypeError(`'${type}' is not a valid enumeration value for ReadableStreamType`)
      const st = {
        source: underlyingSource,
        strategy,
        type,
        highWaterMark: highWater(strategy, type === "bytes" ? 0 : 1),
        queue: [],
        queueSize: 0,
        reads: [],
        reader: null,
        state: "readable",
        storedError: undefined,
        started: false,
        pulling: false,
        pullAgain: false,
        closeRequested: false,
        disturbed: false,
        byobReads: type === "bytes" ? [] : null,
        byobRequest: null,
      }
      st.controller = type === "bytes" ? new ReadableByteStreamController(st, kStream) : new ReadableStreamDefaultController(st, kStream)
      this[kStream] = st
      const startResult = underlyingSource.start?.(st.controller)
      Promise.resolve(startResult)
        .then(
          () => {
            st.started = true
            pullIfNeeded(st)
          },
          (e) => errorStream(st, e),
        )
    }
    get locked() {
      return this[kStream].reader !== null
    }
    get [kDisturbed]() {
      return this[kStream].disturbed
    }
    get [kErrored]() {
      return this[kStream].state === "errored"
    }
    getReader(options) {
      if (options?.mode !== undefined) {
        if (String(options.mode) !== "byob") throw new TypeError(`'${options.mode}' is not a valid enumeration value for ReadableStreamReaderMode`)
        return new ReadableStreamBYOBReader(this)
      }
      return new ReadableStreamDefaultReader(this)
    }
    cancel(reason) {
      if (this.locked) return Promise.reject(new TypeError("Cannot cancel a locked ReadableStream"))
      return cancelStream(this[kStream], reason)
    }
    async pipeTo(dest, options = {}) {
      if (!(dest instanceof WritableStream)) throw new TypeError("pipeTo destination must be a WritableStream")
      if (this.locked) throw new TypeError("ReadableStream is locked")
      if (dest.locked) throw new TypeError("WritableStream is locked")
      const reader = this.getReader()
      const writer = dest.getWriter()
      const signal = options.signal
      let aborted = null
      const onAbort = () => {
        aborted = signal.reason
      }
      if (signal) {
        if (signal.aborted) aborted = signal.reason
        else signal.addEventListener("abort", onAbort, { once: true })
      }
      try {
        while (true) {
          if (aborted !== null) {
            const err = aborted
            if (!options.preventAbort) await writer.abort(err).catch(() => {})
            if (!options.preventCancel) await reader.cancel(err).catch(() => {})
            throw err
          }
          await writer.ready
          const { value, done } = await reader.read()
          if (done) {
            if (!options.preventClose) await writer.close()
            return
          }
          try {
            await writer.write(value)
          } catch (e) {
            if (!options.preventCancel) await reader.cancel(e).catch(() => {})
            throw e
          }
        }
      } catch (e) {
        if (this[kStream].state === "errored" && !options.preventAbort) await writer.abort(e).catch(() => {})
        throw e
      } finally {
        if (signal) signal.removeEventListener("abort", onAbort)
        reader.releaseLock()
        writer.releaseLock()
      }
    }
    pipeThrough(transform, options) {
      if (!transform || !(transform.readable instanceof ReadableStream) || !(transform.writable instanceof WritableStream)) throw new TypeError("pipeThrough needs a pair of readable and writable streams")
      if (this.locked) throw new TypeError("ReadableStream is locked")
      this.pipeTo(transform.writable, options).catch(() => {})
      return transform.readable
    }
    tee() {
      if (this.locked) throw new TypeError("ReadableStream is locked")
      const reader = this.getReader()
      const ctrls = []
      const cancelled = [false, false]
      let reading = false
      const pull = () => {
        if (reading) return Promise.resolve()
        reading = true
        return reader.read().then(
          ({ value, done }) => {
            reading = false
            if (done) {
              for (const c of ctrls) {
                try {
                  c.close()
                } catch {}
              }
              return
            }
            ctrls.forEach((c, i) => {
              if (cancelled[i]) return
              try {
                c.enqueue(value)
              } catch {}
            })
          },
          (e) => {
            reading = false
            for (const c of ctrls) c.error(e)
          },
        )
      }
      const make = (i) =>
        new ReadableStream({
          start(c) {
            ctrls[i] = c
          },
          pull,
          cancel(reason) {
            cancelled[i] = true
            if (cancelled[0] && cancelled[1]) return reader.cancel(reason)
          },
        })
      return [make(0), make(1)]
    }
    values(options) {
      const reader = this.getReader()
      const preventCancel = !!options?.preventCancel
      const iterator = {
        next: () =>
          reader.read().then((r) => {
            if (r.done) reader.releaseLock()
            return r
          }),
        return: (value) => {
          const p = preventCancel ? Promise.resolve() : reader.cancel(value)
          return p.then(() => {
            reader.releaseLock()
            return { value, done: true }
          })
        },
        [Symbol.asyncIterator]() {
          return this
        },
      }
      return iterator
    }
    [Symbol.asyncIterator]() {
      return this.values()
    }
    static from(iterable) {
      if (iterable instanceof ReadableStream) return iterable
      const it = iterable?.[Symbol.asyncIterator]?.() ?? iterable?.[Symbol.iterator]?.()
      if (!it) throw new TypeError("ReadableStream.from requires an iterable")
      return new ReadableStream({
        async pull(controller) {
          const { value, done } = await it.next()
          if (done) controller.close()
          else controller.enqueue(value)
        },
        async cancel(reason) {
          await it.return?.(reason)
        },
      })
    }
    get [Symbol.toStringTag]() {
      return "ReadableStream"
    }
  }

  class WritableStreamDefaultController {
    #st
    constructor(st, key) {
      if (key !== kStream) throw new TypeError("Illegal constructor")
      this.#st = st
    }
    get signal() {
      return this.#st.abortController.signal
    }
    error(e) {
      const st = this.#st
      if (st.state !== "writable") return
      failWritable(st, e)
    }
  }
  function failWritable(st, e) {
    if (st.state === "errored") return
    st.state = "errored"
    st.storedError = e
    for (const w of st.queue.splice(0)) w.reject(e)
    if (st.closing) st.closing.reject(e)
    st.ready.reject(e)
    if (st.writer) st.writer[kStream].closed.reject(e)
  }
  function advanceWritable(st) {
    if (st.inflight || !st.started || st.state === "errored") return
    const next = st.queue.shift()
    if (!next) {
      if (st.closing && st.state === "writable") {
        st.state = "closing"
        st.inflight = true
        Promise.resolve()
          .then(() => st.sink.close?.())
          .then(
            () => {
              st.inflight = false
              st.state = "closed"
              st.closing.resolve()
              if (st.writer) st.writer[kStream].closed.resolve()
            },
            (e) => {
              st.inflight = false
              failWritable(st, e)
            },
          )
      }
      return
    }
    st.inflight = true
    Promise.resolve()
      .then(() => st.sink.write?.(next.chunk, st.controller))
      .then(
        () => {
          st.inflight = false
          st.queueSize -= next.size
          next.resolve()
          if (st.highWaterMark - st.queueSize > 0 && st.backpressure) {
            st.backpressure = false
            st.ready.resolve()
          }
          advanceWritable(st)
        },
        (e) => {
          st.inflight = false
          next.reject(e)
          failWritable(st, e)
        },
      )
  }

  class WritableStreamDefaultWriter {
    constructor(stream) {
      if (!(stream instanceof WritableStream)) throw new TypeError("WritableStreamDefaultWriter constructor argument must be a WritableStream")
      const st = stream[kStream]
      if (st.writer) throw new TypeError("WritableStream is locked")
      this[kStream] = { stream, st, closed: defer() }
      st.writer = this
      if (st.state === "closed") this[kStream].closed.resolve()
      if (st.state === "errored") this[kStream].closed.reject(st.storedError)
    }
    get closed() {
      return this[kStream].closed.promise
    }
    get ready() {
      const { st } = this[kStream]
      return st ? st.ready.promise : Promise.reject(new TypeError("Writer was released"))
    }
    get desiredSize() {
      const { st } = this[kStream]
      if (!st) throw new TypeError("Writer was released")
      if (st.state === "errored") return null
      if (st.state === "closed" || st.state === "closing") return 0
      return st.highWaterMark - st.queueSize
    }
    write(chunk) {
      const { st } = this[kStream]
      if (!st) return Promise.reject(new TypeError("Writer was released"))
      if (st.state === "errored") return Promise.reject(st.storedError)
      if (st.closing || st.state !== "writable") return Promise.reject(new TypeError("The stream is closing or closed and cannot be written to"))
      let size
      try {
        size = chunkSize(st.strategy, chunk)
      } catch (e) {
        failWritable(st, e)
        return Promise.reject(e)
      }
      const d = defer()
      st.queue.push({ chunk, size, resolve: d.resolve, reject: d.reject })
      st.queueSize += size
      if (st.highWaterMark - st.queueSize <= 0 && !st.backpressure) {
        st.backpressure = true
        st.ready = defer()
      }
      advanceWritable(st)
      return d.promise
    }
    close() {
      const { st } = this[kStream]
      if (!st) return Promise.reject(new TypeError("Writer was released"))
      if (st.state === "errored") return Promise.reject(st.storedError)
      if (st.closing || st.state !== "writable") return Promise.reject(new TypeError("The stream is already closing"))
      st.closing = defer()
      advanceWritable(st)
      return st.closing.promise
    }
    abort(reason) {
      const { st } = this[kStream]
      if (!st) return Promise.reject(new TypeError("Writer was released"))
      return abortWritable(st, reason)
    }
    releaseLock() {
      const { st } = this[kStream]
      if (!st) return
      const e = new TypeError("Writer was released")
      if (st.state === "writable") {
        this[kStream].closed.reject(e)
        if (st.backpressure) st.ready.reject(e)
      }
      st.writer = null
      st.ready = defer()
      st.ready.resolve()
      this[kStream].st = null
    }
  }
  function abortWritable(st, reason) {
    if (st.state === "closed" || st.state === "errored") return Promise.resolve()
    st.abortController.abort(reason)
    const err = reason === undefined ? new DOMException("The operation was aborted", "AbortError") : reason
    failWritable(st, err)
    return Promise.resolve()
      .then(() => st.sink.abort?.(reason))
      .then(() => undefined)
  }

  class WritableStream {
    constructor(underlyingSink = {}, strategy = {}) {
      if (underlyingSink === null) underlyingSink = {}
      if (underlyingSink.type !== undefined) throw new RangeError("Invalid type is specified")
      const st = {
        sink: underlyingSink,
        strategy,
        highWaterMark: highWater(strategy, 1),
        queue: [],
        queueSize: 0,
        writer: null,
        state: "writable",
        storedError: undefined,
        started: false,
        inflight: false,
        closing: null,
        backpressure: false,
        ready: defer(),
        abortController: new AbortController(),
      }
      st.ready.resolve()
      st.controller = new WritableStreamDefaultController(st, kStream)
      this[kStream] = st
      const startResult = underlyingSink.start?.(st.controller)
      Promise.resolve(startResult)
        .then(
          () => {
            st.started = true
            advanceWritable(st)
          },
          (e) => failWritable(st, e),
        )
    }
    get locked() {
      return this[kStream].writer !== null
    }
    getWriter() {
      return new WritableStreamDefaultWriter(this)
    }
    abort(reason) {
      if (this.locked) return Promise.reject(new TypeError("Cannot abort a locked WritableStream"))
      return abortWritable(this[kStream], reason)
    }
    close() {
      if (this.locked) return Promise.reject(new TypeError("Cannot close a locked WritableStream"))
      const st = this[kStream]
      if (st.closing || st.state !== "writable") return Promise.reject(new TypeError("The stream is already closing"))
      st.closing = defer()
      advanceWritable(st)
      return st.closing.promise
    }
    get [Symbol.toStringTag]() {
      return "WritableStream"
    }
  }

  class TransformStreamDefaultController {
    #st
    constructor(st, key) {
      if (key !== kStream) throw new TypeError("Illegal constructor")
      this.#st = st
    }
    get desiredSize() {
      return this.#st.readableController.desiredSize
    }
    enqueue(chunk) {
      try {
        this.#st.readableController.enqueue(chunk)
      } catch (e) {
        this.#st.writableController.error(e)
        throw e
      }
    }
    error(e) {
      this.#st.readableController.error(e)
      this.#st.writableController.error(e)
    }
    terminate() {
      try {
        this.#st.readableController.close()
      } catch {}
      this.#st.writableController.error(new TypeError("The transform stream has been terminated"))
    }
  }

  class TransformStream {
    #readable
    #writable
    constructor(transformer = {}, writableStrategy = {}, readableStrategy = {}) {
      if (transformer === null) transformer = {}
      if (transformer.readableType !== undefined || transformer.writableType !== undefined) throw new RangeError("Invalid type is specified")
      const st = {}
      const controller = new TransformStreamDefaultController(st, kStream)
      const transform = typeof transformer.transform === "function" ? (chunk) => transformer.transform(chunk, controller) : (chunk) => controller.enqueue(chunk)
      let startPromise
      this.#readable = new ReadableStream(
        {
          start(c) {
            st.readableController = c
          },
          pull() {
            if (st.pullWaiting) {
              const w = st.pullWaiting
              st.pullWaiting = null
              w()
            }
          },
          cancel(reason) {
            st.writableController.error(reason)
          },
        },
        readableStrategy,
      )
      this.#writable = new WritableStream(
        {
          start(c) {
            st.writableController = c
            startPromise = Promise.resolve().then(() => transformer.start?.(controller))
            return startPromise
          },
          async write(chunk) {
            if (st.readableController.desiredSize !== null && st.readableController.desiredSize <= 0) {
              await new Promise((resolve) => (st.pullWaiting = resolve))
            }
            await transform(chunk)
          },
          async close() {
            await transformer.flush?.(controller)
            try {
              st.readableController.close()
            } catch {}
          },
          async abort(reason) {
            await transformer.cancel?.(reason)
            st.readableController.error(reason)
          },
        },
        writableStrategy,
      )
    }
    get readable() {
      return this.#readable
    }
    get writable() {
      return this.#writable
    }
    get [Symbol.toStringTag]() {
      return "TransformStream"
    }
  }

  const utf8Tail = (bytes) => {
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

  class TextEncoderStream extends TransformStream {
    constructor() {
      let pendingHigh = null
      super({
        transform(chunk, controller) {
          let s = String(chunk)
          if (pendingHigh !== null) {
            s = pendingHigh + s
            pendingHigh = null
          }
          const last = s.charCodeAt(s.length - 1)
          if (last >= 0xd800 && last <= 0xdbff) {
            pendingHigh = s[s.length - 1]
            s = s.slice(0, -1)
          }
          if (s) controller.enqueue(encoder.encode(s))
        },
        flush(controller) {
          if (pendingHigh !== null) controller.enqueue(encoder.encode(pendingHigh))
        },
      })
    }
    get encoding() {
      return "utf-8"
    }
  }

  class TextDecoderStream extends TransformStream {
    #encoding
    #fatal
    #ignoreBOM
    constructor(label = "utf-8", options = {}) {
      const td = new TextDecoder(label, options)
      let pending = new Uint8Array(0)
      let first = true
      super({
        transform(chunk, controller) {
          let bytes = chunk instanceof Uint8Array ? chunk : chunk instanceof ArrayBuffer ? new Uint8Array(chunk) : viewBytes(chunk)
          if (pending.length) {
            const joined = new Uint8Array(pending.length + bytes.length)
            joined.set(pending)
            joined.set(bytes, pending.length)
            bytes = joined
            pending = new Uint8Array(0)
          }
          const keep = td.encoding === "utf-8" ? utf8Tail(bytes) : td.encoding === "utf-16le" ? bytes.length % 2 : 0
          if (keep) {
            pending = bytes.slice(bytes.length - keep)
            bytes = bytes.subarray(0, bytes.length - keep)
          }
          if (first && !options.ignoreBOM && td.encoding === "utf-8" && bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) bytes = bytes.subarray(3)
          if (bytes.length) {
            first = false
            controller.enqueue(td.decode(bytes))
          }
        },
        flush(controller) {
          if (pending.length) controller.enqueue(td.decode(pending))
        },
      })
      this.#encoding = td.encoding
      this.#fatal = !!options.fatal
      this.#ignoreBOM = !!options.ignoreBOM
    }
    get encoding() {
      return this.#encoding
    }
    get fatal() {
      return this.#fatal
    }
    get ignoreBOM() {
      return this.#ignoreBOM
    }
  }

  const zlibFor = (format, decompress) => {
    const zlib = require("node:zlib")
    const op = { gzip: decompress ? "gunzipSync" : "gzipSync", deflate: decompress ? "inflateSync" : "deflateSync", "deflate-raw": decompress ? "inflateRawSync" : "deflateRawSync" }[format]
    if (!op) throw new TypeError(`Unsupported compression format: '${format}'`)
    return zlib[op]
  }
  class CompressionStream extends TransformStream {
    #format
    constructor(format) {
      const run = zlibFor(String(format), false)
      const chunks = []
      super({
        transform(chunk) {
          chunks.push(chunk instanceof Uint8Array ? chunk : chunk instanceof ArrayBuffer ? new Uint8Array(chunk) : viewBytes(chunk))
        },
        flush(controller) {
          const out = run(concatBytes(chunks))
          controller.enqueue(new Uint8Array(out.buffer, out.byteOffset, out.byteLength))
        },
      })
      this.#format = String(format)
    }
    get format() {
      return this.#format
    }
  }
  class DecompressionStream extends TransformStream {
    #format
    constructor(format) {
      const run = zlibFor(String(format), true)
      const chunks = []
      super({
        transform(chunk) {
          chunks.push(chunk instanceof Uint8Array ? chunk : chunk instanceof ArrayBuffer ? new Uint8Array(chunk) : viewBytes(chunk))
        },
        flush(controller) {
          const out = run(concatBytes(chunks))
          controller.enqueue(new Uint8Array(out.buffer, out.byteOffset, out.byteLength))
        },
      })
      this.#format = String(format)
    }
    get format() {
      return this.#format
    }
  }

  async function collectStream(stream) {
    const chunks = []
    for await (const c of stream) chunks.push(typeof c === "string" ? encoder.encode(c) : c instanceof Uint8Array ? c : c instanceof ArrayBuffer ? new Uint8Array(c) : viewBytes(c))
    return concatBytes(chunks)
  }
  const bytesToStream = (bytes) =>
    new ReadableStream({
      type: "bytes",
      start(c) {
        if (bytes.length) c.enqueue(bytes)
        c.close()
      },
    })

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
      return bytesToStream(this[kBytes].slice())
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
    if (body instanceof ReadableStream) return body
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

  const kRaw = Symbol("rawBody")
  const kRawRequest = Symbol("rawRequest")
  class Body {
    #body
    #used = false
    #stream = null
    constructor(body) {
      this.#body = bodyInit(body)
    }
    get [kRaw]() {
      return this.#body
    }
    set [kRaw](v) {
      this.#body = v
      this.#stream = null
    }
    get body() {
      if (this.#body === null) return null
      if (this.#body instanceof ReadableStream) return this.#body
      if (!this.#stream) this.#stream = bytesToStream(toBytes(this.#body))
      return this.#stream
    }
    get bodyUsed() {
      if (this.#used) return true
      const s = this.#body instanceof ReadableStream ? this.#body : this.#stream
      return s ? s[kDisturbed] : false
    }
    async #take() {
      if (this.bodyUsed) throw new TypeError("Body is unusable: Body has already been read")
      this.#used = true
      const b = this.#body
      if (b instanceof ReadableStream) return await collectStream(b)
      return b
    }
    async text() {
      const b = await this.#take()
      return typeof b === "string" ? b : b === null ? "" : decoder.decode(b)
    }
    async json() {
      return JSON.parse(await this.text())
    }
    async bytes() {
      return new Uint8Array(await this.arrayBuffer())
    }
    async arrayBuffer() {
      const b = toBytes(await this.#take())
      return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength)
    }
    async blob() {
      const b = toBytes(await this.#take())
      const type = this.headers?.get("content-type") ?? ""
      return new Blob([b], { type })
    }
    async formData() {
      const type = this.headers?.get("content-type") ?? ""
      const b = toBytes(await this.#take())
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
    #status
    #statusText
    #headers
    #redirected
    #type
    #url
    constructor(body = null, init = {}) {
      super(body)
      init ??= {}
      const status = init.status ?? 200
      if (!internal && (status < 200 || status > 599)) {
        throw new RangeError(`init["status"] must be in the range of 200 to 599, inclusive.`)
      }
      this.#status = status
      this.#statusText = init.statusText ?? ""
      this.#headers = init.headers instanceof Headers && internal ? init.headers : new Headers(init.headers)
      if (!internal && body != null && !this.#headers.has("content-type")) {
        const t = defaultType(body)
        if (t) this.#headers.set("content-type", t)
      }
      this.#redirected = init.redirected ?? false
      this.#type = internal ? "basic" : "default"
      this.#url = init.url ?? ""
    }
    get status() {
      return this.#status
    }
    get statusText() {
      return this.#statusText
    }
    get headers() {
      return this.#headers
    }
    get ok() {
      return this.#status >= 200 && this.#status < 300
    }
    get redirected() {
      return this.#redirected
    }
    get type() {
      return this.#type
    }
    get url() {
      return this.#url
    }
    clone() {
      if (this.bodyUsed) throw new TypeError("Response.clone: Body has already been consumed.")
      let raw = this[kRaw]
      if (raw instanceof ReadableStream) {
        const [a, b] = raw.tee()
        this[kRaw] = a
        raw = b
      }
      internal = true
      let copy
      try {
        copy = new Response(raw, {
          status: this.#status,
          statusText: this.#statusText,
          headers: new Headers(this.#headers),
          url: this.#url,
          redirected: this.#redirected,
        })
      } finally {
        internal = false
      }
      copy.#type = this.#type
      return copy
    }
    static json(data, init = {}) {
      const headers = new Headers(init?.headers)
      if (!headers.has("content-type")) headers.set("content-type", "application/json")
      return new Response(JSON.stringify(data), { ...init, headers })
    }
    static redirect(url, status = 302) {
      if (![301, 302, 303, 307, 308].includes(status)) throw new RangeError("Invalid status code")
      return new Response(null, { status, headers: { location: String(url) } })
    }
    static error() {
      internal = true
      let r
      try {
        r = new Response(null, { status: 0 })
      } finally {
        internal = false
      }
      r.#type = "error"
      return r
    }
  }

  class Request extends Body {
    #method
    #url
    #headers
    #redirect
    #signal
    #mode
    #credentials
    #cache
    #referrer
    #referrerPolicy
    #integrity
    #keepalive
    constructor(input, init = {}) {
      init ??= {}
      const src = input instanceof Request ? input : null
      const method = String(init.method ?? src?.method ?? "GET").toUpperCase()
      const body = init.body !== undefined ? init.body : src ? src[kRaw] : null
      if ((method === "GET" || method === "HEAD") && body != null) {
        throw new TypeError("Request with GET/HEAD method cannot have body.")
      }
      super(body)
      this.#method = method
      this.#url = src ? src.url : internal ? String(input) : new URL(String(input)).href
      this.#headers = new Headers(init.headers ?? src?.headers)
      if (body != null && !this.#headers.has("content-type")) {
        const t = defaultType(body)
        if (t) this.#headers.set("content-type", t)
      }
      this.#redirect = init.redirect ?? src?.redirect ?? "follow"
      this.#signal = init.signal ?? src?.signal ?? new AbortSignal(SIGNAL_KEY)
      this.#mode = init.mode ?? src?.mode ?? "cors"
      this.#credentials = init.credentials ?? src?.credentials ?? "same-origin"
      this.#cache = init.cache ?? src?.cache ?? "default"
      this.#referrer = init.referrer === "" ? "" : init.referrer ?? src?.referrer ?? "about:client"
      this.#referrerPolicy = init.referrerPolicy ?? src?.referrerPolicy ?? ""
      this.#integrity = init.integrity ?? src?.integrity ?? ""
      this.#keepalive = Boolean(init.keepalive ?? src?.keepalive ?? false)
    }
    get method() {
      return this.#method
    }
    get url() {
      return this.#url
    }
    get headers() {
      return this.#headers
    }
    get redirect() {
      return this.#redirect
    }
    get signal() {
      return this.#signal
    }
    get destination() {
      return ""
    }
    get referrer() {
      return this.#referrer
    }
    get referrerPolicy() {
      return this.#referrerPolicy
    }
    get mode() {
      return this.#mode
    }
    get credentials() {
      return this.#credentials
    }
    get cache() {
      return this.#cache
    }
    get integrity() {
      return this.#integrity
    }
    get keepalive() {
      return this.#keepalive
    }
    get isReloadNavigation() {
      return false
    }
    get isHistoryNavigation() {
      return false
    }
    get duplex() {
      return "half"
    }
    clone() {
      if (this.bodyUsed) throw new TypeError("Request.clone: Body has already been consumed.")
      return new Request(this)
    }
  }

  const blobUrls = new Map()
  const describeArg = (v) => {
    if (v === null) return "null"
    if (v === undefined) return "undefined"
    if (typeof v === "function") return `function ${v.name}`
    if (typeof v === "object") return v.constructor?.name ? `an instance of ${v.constructor.name}` : "an instance of Object"
    const shown = typeof v === "string" ? `'${v.length > 28 ? v.slice(0, 25) + "..." : v}'` : String(v)
    return `type ${typeof v} (${shown})`
  }
  Object.defineProperties(URL, {
    createObjectURL: {
      value: function createObjectURL(obj) {
        if (!(obj instanceof Blob)) {
          const e = new TypeError(`The "obj" argument must be an instance of Blob. Received ${describeArg(obj)}`)
          e.code = "ERR_INVALID_ARG_TYPE"
          throw e
        }
        const id = `blob:nodedata:${globalThis.crypto.randomUUID()}`
        blobUrls.set(id, obj)
        return id
      },
      writable: true,
      enumerable: true,
      configurable: true,
    },
    revokeObjectURL: {
      value: function revokeObjectURL(url) {
        if (arguments.length === 0) {
          const e = new TypeError('The "url" argument must be specified')
          e.code = "ERR_MISSING_ARGS"
          throw e
        }
        blobUrls.delete(String(url))
      },
      writable: true,
      enumerable: true,
      configurable: true,
    },
    [Symbol.for("jsq.resolveObjectURL")]: {
      value: (url) => {
        const blob = blobUrls.get(String(url))
        return blob === undefined ? undefined : blob.slice(0, blob.size, blob.type)
      },
    },
  })
  for (const name of ["parse", "canParse"]) Object.defineProperty(URL, name, { enumerable: true })
  const percentDecode = (s) => {
    const out = []
    for (let i = 0; i < s.length; i++) {
      const c = s.charCodeAt(i)
      if (c === 37 && /^[0-9a-fA-F]{2}$/.test(s.slice(i + 1, i + 3))) {
        out.push(parseInt(s.slice(i + 1, i + 3), 16))
        i += 2
      } else if (c < 128) {
        out.push(c)
      } else {
        for (const b of encoder.encode(s[i])) out.push(b)
      }
    }
    return new Uint8Array(out)
  }
  const dataUrlResponse = (href) => {
    const rest = href.slice(5)
    const comma = rest.indexOf(",")
    if (comma < 0) throw new TypeError("fetch failed", { cause: new Error("invalid data: URL") })
    let mime = rest.slice(0, comma).replace(/^[\t\n\f\r ]+|[\t\n\f\r ]+$/g, "")
    let body = percentDecode(rest.slice(comma + 1))
    const b64 = /;[\t\n\f\r ]*base64$/i.exec(mime)
    if (b64) {
      mime = mime.slice(0, b64.index)
      const text = String.fromCharCode(...body).replace(/[\t\n\f\r ]+/g, "")
      try {
        body = Uint8Array.from(atob(text), (ch) => ch.charCodeAt(0))
      } catch {
        throw new TypeError("fetch failed", { cause: new Error("invalid base64 in data: URL") })
      }
    }
    if (mime.startsWith(";")) mime = "text/plain" + mime
    const [essence, ...params] = mime.split(";")
    const valid = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+\/[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(essence.trim())
    const contentType = valid ? [essence.trim().toLowerCase(), ...params.map((p) => p.trim())].join(";") : "text/plain;charset=US-ASCII"
    internal = true
    try {
      return new Response(body, { status: 200, statusText: "OK", headers: { "content-type": contentType }, url: href, redirected: false })
    } finally {
      internal = false
    }
  }

  async function fetch(input, init = {}) {
    const req = new Request(input, init)
    const url = new URL(req.url)
    if (url.protocol === "data:") {
      req.signal.throwIfAborted()
      return dataUrlResponse(url.href)
    }
    if (url.protocol === "blob:") {
      req.signal.throwIfAborted()
      const blob = blobUrls.get(url.href)
      if (!blob || req.method !== "GET") throw new TypeError("fetch failed", { cause: new Error(blob ? "invalid method" : "invalid blob: URL") })
      internal = true
      try {
        return new Response(blob, { status: 200, statusText: "OK", headers: { "content-type": blob.type, "content-length": String(blob.size) }, url: url.href, redirected: false })
      } finally {
        internal = false
      }
    }
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      throw new TypeError("fetch failed", { cause: new Error(`unsupported protocol ${url.protocol}`) })
    }
    const signal = req.signal
    signal.throwIfAborted()
    let payload = req[kRaw]
    if (payload instanceof ReadableStream) payload = await collectStream(payload)
    const [id, pending] = native.fetch(
      req.method,
      url.href,
      [...req.headers],
      payload,
      Number(init.timeout ?? 0),
      req.redirect !== "manual",
      init.tls ?? null,
      init[Symbol.for("jsq.rawBody")] !== true,
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
      native.abort(id)
      throw new TypeError("fetch failed", { cause: new Error("unexpected redirect") })
    }
    const empty = raw.status === 204 || raw.status === 304 || req.method === "HEAD"
    let body = null
    if (empty) {
      native.abort(id)
    } else {
      let controller
      let finished = false
      const onAbort = () => {
        if (finished) return
        finished = true
        native.abort(id)
        try {
          controller.error(signal.reason)
        } catch {}
      }
      body = new ReadableStream({
        type: "bytes",
        start(c) {
          controller = c
        },
        cancel() {
          finished = true
          signal.removeEventListener("abort", onAbort)
          native.abort(id)
        },
      })
      signal.addEventListener("abort", onAbort, { once: true })
      native.fetchBody(
        id,
        (chunk) => {
          if (!finished && chunk.byteLength) controller.enqueue(chunk)
        },
        (error) => {
          if (finished) return
          finished = true
          signal.removeEventListener("abort", onAbort)
          if (error === undefined) controller.close()
          else controller.error(new TypeError("terminated", { cause: new Error(error) }))
        },
      )
    }
    internal = true
    try {
      return new Response(body, {
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

  async function toReply(res) {
    if (res instanceof Response) {
      return [res.status, [...res.headers], res[kRaw] ?? ""]
    }
    if (res === undefined || res === null) return [204, [], ""]
    if (typeof res === "string") return [200, [["content-type", "text/plain;charset=UTF-8"]], res]
    return [200, [["content-type", "application/json"]], JSON.stringify(res)]
  }

  const chunkBytes = (c) => (typeof c === "string" ? encoder.encode(c) : c instanceof Uint8Array ? c : c instanceof ArrayBuffer ? new Uint8Array(c) : ArrayBuffer.isView(c) ? viewBytes(c) : encoder.encode(String(c)))
  async function pump(replyId, status, headers, stream) {
    native.replyHead(replyId, status, headers)
    let reader
    try {
      reader = stream.getReader()
      for (;;) {
        const { value, done } = await reader.read()
        if (done) break
        if (!native.replyChunk(replyId, chunkBytes(value))) {
          reader.cancel().catch(() => {})
          break
        }
      }
      native.replyEnd(replyId, false)
    } catch (err) {
      native.replyEnd(replyId, true)
    }
  }
  const sendReply = (replyId, status, headers, body, chunked = false) => {
    if (body instanceof ReadableStream) return pump(replyId, status, headers, body)
    native.reply(replyId, status, headers, typeof body === "string" ? body : toBytes(body), chunked)
  }
  const nativeListenPath = (path, upgrade, dispatch) => {
    const id = native.servePath(path, dispatch, typeof upgrade === "function" ? upgrade : null)
    let open = true
    return {
      path,
      stop() {
        if (open) {
          open = false
          native.close(id)
        }
      },
    }
  }
  const nativeListen = (options, dispatch) => {
    const tls = options.tls ?? null
    const clientCa = tls?.requestCert ? tls.ca ?? true : null
    const [id, port, hostname] = native.serve(String(options.hostname ?? "127.0.0.1"), Number(options.port ?? 3000), dispatch, typeof options.upgrade === "function" ? options.upgrade : null, tls?.key ?? null, tls?.cert ?? null, clientCa, tls?.rejectUnauthorized !== false)
    let open = true
    return {
      hostname,
      port,
      stop() {
        if (open) {
          open = false
          native.close(id)
        }
      },
    }
  }

  function serve(options, handler) {
    if (typeof options === "function") [options, handler] = [{}, options]
    options ??= {}
    handler ??= options.fetch
    if (typeof handler !== "function") throw new TypeError("serve() needs a handler function")
    const onError = options.error
    const tls = options.tls ?? null
    const [id, port, address] = native.serve(String(options.hostname ?? "127.0.0.1"), Number(options.port ?? 3000), dispatch, typeof options.upgrade === "function" ? options.upgrade : null, tls?.key ?? null, tls?.cert ?? null)
    let open = true
    const server = {
      hostname: address,
      port,
      url: new URL(`${tls ? "https" : "http"}://${address.includes(":") ? `[${address}]` : address}:${port}/`),
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
    const closers = new Map()
    function dispatch(raw, replyId) {
      if (raw === null) {
        closers.get(replyId)?.()
        return
      }
      server.pendingRequests++
      const aborter = new AbortController()
      closers.set(replyId, () => aborter.abort())
      internal = true
      let req
      try {
        const hasBody = raw.body.length > 0 && raw.method !== "GET" && raw.method !== "HEAD"
        req = new Request(raw.url, { method: raw.method, headers: raw.headers, body: hasBody ? raw.body : null, signal: aborter.signal })
        req.remoteAddress = raw.remote
        req[kRawRequest] = raw
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
        .then(async ([status, headers, body]) => {
          await sendReply(replyId, status, headers, body)
          server.pendingRequests--
          closers.delete(replyId)
        })
    }
    return server
  }

  const kEvents = Symbol("messageEvents")
  const eventList = (target, event, create) => {
    let table = target[kEvents]
    if (!table) {
      if (!create) return undefined
      table = Object.create(null)
      Object.defineProperty(target, kEvents, { value: table, configurable: true, writable: true })
    }
    if (create) return (table[event] ??= [])
    return table[event]
  }
  const onceWrapper = (target, event, fn) => {
    const wrapper = function (...args) {
      target.off(event, wrapper)
      return fn.apply(this, args)
    }
    wrapper.listener = fn
    return wrapper
  }
  const messageEmitter = {
    on(event, fn) {
      eventList(this, event, true).push(fn)
      return this
    },
    addListener(event, fn) {
      return this.on(event, fn)
    },
    prependListener(event, fn) {
      eventList(this, event, true).unshift(fn)
      return this
    },
    once(event, fn) {
      return this.on(event, onceWrapper(this, event, fn))
    },
    prependOnceListener(event, fn) {
      return this.prependListener(event, onceWrapper(this, event, fn))
    },
    off(event, fn) {
      const list = eventList(this, event, false)
      if (list) {
        const i = list.findIndex((f) => f === fn || f.listener === fn)
        if (i >= 0) list.splice(i, 1)
      }
      return this
    },
    removeListener(event, fn) {
      return this.off(event, fn)
    },
    removeAllListeners(event) {
      const table = this[kEvents]
      if (table) {
        if (event === undefined) for (const k of Reflect.ownKeys(table)) delete table[k]
        else delete table[event]
      }
      return this
    },
    emit(event, ...args) {
      const list = eventList(this, event, false)
      if (!list || list.length === 0) return false
      for (const fn of list.slice()) fn.apply(this, args)
      return true
    },
    listenerCount(event) {
      return eventList(this, event, false)?.length ?? 0
    },
    listeners(event) {
      return (eventList(this, event, false) ?? []).map((f) => f.listener ?? f)
    },
    rawListeners(event) {
      return (eventList(this, event, false) ?? []).slice()
    },
    eventNames() {
      const table = this[kEvents]
      return table ? Reflect.ownKeys(table).filter((k) => table[k].length > 0) : []
    },
    setMaxListeners() {
      return this
    },
    getMaxListeners() {
      return 10
    },
  }
  const defineMethods = (proto, methods) => {
    for (const key of Reflect.ownKeys(methods)) Object.defineProperty(proto, key, { value: methods[key], writable: true, configurable: true })
  }
  const kPeerCert = Symbol("peerCert")
  function NullSocket(remoteAddress, peerCert) {
    this.remoteAddress = remoteAddress
    this.authorized = peerCert !== undefined
    this.authorizationError = null
    Object.defineProperty(this, kPeerCert, { value: peerCert, configurable: true })
    this.remotePort = 0
    this.localAddress = "127.0.0.1"
    this.localPort = 0
    this.encrypted = false
    this.readable = true
    this.writable = true
    this.destroyed = false
  }
  defineMethods(NullSocket.prototype, {
    getPeerCertificate() {
      const raw = this[kPeerCert]
      if (raw === undefined) return {}
      const { X509Certificate } = require("node:crypto")
      return new X509Certificate(Buffer.from(raw.buffer, raw.byteOffset, raw.byteLength)).toLegacyObject()
    },
    setTimeout() {
      return this
    },
    setNoDelay() {
      return this
    },
    setKeepAlive() {
      return this
    },
    setEncoding() {
      return this
    },
    on() {
      return this
    },
    once() {
      return this
    },
    off() {
      return this
    },
    addListener() {
      return this
    },
    removeListener() {
      return this
    },
    removeAllListeners() {
      return this
    },
    emit() {
      return false
    },
    listenerCount() {
      return 0
    },
    ref() {
      return this
    },
    unref() {
      return this
    },
    pause() {
      return this
    },
    resume() {
      return this
    },
    cork() {},
    uncork() {},
    write() {
      return true
    },
    end() {
      this.writable = false
      return this
    },
    destroySoon() {
      this.writable = false
    },
    destroy() {
      this.destroyed = true
      return this
    },
    address: () => ({ address: "127.0.0.1", family: "IPv4", port: 0 }),
  })
  const nullSocket = (remoteAddress, peerCert) => new NullSocket(remoteAddress, peerCert)
  const SINGLE_HEADERS = new Set(["age", "authorization", "content-length", "content-type", "etag", "expires", "from", "host", "if-modified-since", "if-unmodified-since", "last-modified", "location", "max-forwards", "proxy-authorization", "referer", "retry-after", "server", "user-agent"])
  const nodeHeaders = (pairs) => {
    const headers = {}
    const rawHeaders = new Array(pairs.length * 2)
    for (let i = 0; i < pairs.length; i++) {
      const name = pairs[i][0]
      const value = pairs[i][1]
      rawHeaders[2 * i] = name
      rawHeaders[2 * i + 1] = value
      const key = name.toLowerCase()
      const cur = headers[key]
      if (cur === undefined) headers[key] = key === "set-cookie" ? [value] : value
      else if (key === "set-cookie") cur.push(value)
      else if (!SINGLE_HEADERS.has(key)) headers[key] = key === "cookie" ? `${cur}; ${value}` : `${cur}, ${value}`
    }
    return { headers, rawHeaders }
  }

  const kMessage = Symbol("messageState")
  const kClosed = Symbol("messageClosed")
  const closeMessage = (msg) => {
    if (msg[Symbol.for("jsq.incomingReadableMode")]) {
      if (msg[kClosed]) return
      Object.defineProperty(msg, kClosed, { value: true, configurable: true })
      queueMicrotask(() => {
        msg.destroyed = true
        msg.emit("close")
      })
      return
    }
    const state = msg[kMessage]
    if (!state || state.closed) return
    state.closed = true
    queueMicrotask(() => {
      msg.destroyed = true
      msg.emit("close")
    })
  }
  const kReadableMode = Symbol.for("jsq.incomingReadableMode")
  const kReadableInit = Symbol.for("jsq.incomingReadableInit")
  function IncomingMessage(req) {
    const raw = req?.[kRawRequest]
    const requestLike = !raw && req && typeof req.url === "string" && req.headers && typeof req.headers.entries === "function"
    const init = IncomingMessage[kReadableInit]
    if (typeof init === "function") {
      Object.defineProperty(this, kReadableMode, { value: true, configurable: true })
      this.statusCode = null
      this.statusMessage = null
      this.aborted = false
      this.trailers = {}
      this.rawTrailers = []
      let body = null
      if (raw) {
        const parsed = nodeHeaders(raw.headers)
        this.method = raw.method
        this.url = raw.target
        this.headers = parsed.headers
        this.rawHeaders = parsed.rawHeaders
        this.httpVersion = "1.1"
        this.httpVersionMajor = 1
        this.httpVersionMinor = 1
        this.socket = nullSocket(raw.remote, raw.peerCert)
        body = req[kRaw]
      } else if (requestLike) {
        const u = new URL(req.url)
        this.method = req.method
        this.url = u.pathname + u.search
        this.headers = Object.fromEntries(req.headers)
        this.rawHeaders = [...req.headers].flat()
        this.httpVersion = "1.1"
        this.httpVersionMajor = 1
        this.httpVersionMinor = 1
        this.socket = nullSocket(req.remoteAddress)
        body = req[kRaw]
      } else {
        this.method = null
        this.url = ""
        this.headers = {}
        this.rawHeaders = []
        this.httpVersion = null
        this.httpVersionMajor = null
        this.httpVersionMinor = null
        this.complete = false
        this.socket = req && typeof req === "object" ? req : nullSocket(undefined)
        this.connection = this.socket
        init(this, undefined)
        return
      }
      this.connection = this.socket
      this.complete = true
      init(this, body == null ? new Uint8Array(0) : typeof body === "string" ? encoder.encode(body) : body)
      return
    }
    const state = { bytes: new Uint8Array(0), encoding: null, closed: false }
    Object.defineProperty(this, kMessage, { value: state, configurable: true })
    this.method = "GET"
    this.url = "/"
    this.headers = {}
    this.rawHeaders = []
    this.httpVersion = "1.1"
    this.httpVersionMajor = 1
    this.httpVersionMinor = 1
    this.complete = false
    this.readable = true
    this.aborted = false
    this.destroyed = false
    this.trailers = {}
    this.rawTrailers = []
    if (raw) {
      const parsed = nodeHeaders(raw.headers)
      this.method = raw.method
      this.url = raw.target
      this.headers = parsed.headers
      this.rawHeaders = parsed.rawHeaders
      this.socket = nullSocket(raw.remote, raw.peerCert)
      const b = req[kRaw]
      state.bytes = b == null ? new Uint8Array(0) : typeof b === "string" ? encoder.encode(b) : b
    } else if (req && typeof req.url === "string" && req.headers && typeof req.headers.entries === "function") {
      const u = new URL(req.url)
      this.method = req.method
      this.url = u.pathname + u.search
      this.headers = Object.fromEntries(req.headers)
      this.rawHeaders = [...req.headers].flat()
      this.socket = nullSocket(req.remoteAddress)
      const b = req[kRaw]
      state.bytes = b == null ? new Uint8Array(0) : typeof b === "string" ? encoder.encode(b) : b
    } else {
      this.socket = req && typeof req === "object" ? req : nullSocket(undefined)
    }
    this.connection = this.socket
  }
  const messageState = (msg) => {
    let state = msg[kMessage]
    if (!state) {
      state = { bytes: new Uint8Array(0), encoding: null }
      Object.defineProperty(msg, kMessage, { value: state, configurable: true })
    }
    return state
  }
  const messageChunk = (msg) => {
    const state = messageState(msg)
    if (state.encoding !== null) return decoder.decode(state.bytes)
    return typeof Buffer === "function" ? Buffer.from(state.bytes) : state.bytes
  }
  defineMethods(IncomingMessage.prototype, messageEmitter)
  defineMethods(IncomingMessage.prototype, {
    setEncoding(encoding) {
      messageState(this).encoding = encoding ?? "utf8"
      return this
    },
    on(event, fn) {
      messageEmitter.on.call(this, event, fn)
      const state = messageState(this)
      if (event === "data" && state.bytes.length !== 0) queueMicrotask(() => fn.call(this, messageChunk(this)))
      if (event === "end")
        queueMicrotask(() =>
          queueMicrotask(() => {
            this.complete = true
            this.readable = false
            fn.call(this)
            closeMessage(this)
          }),
        )
      if (event === "close" && state.closed && this.destroyed) queueMicrotask(() => fn.call(this))
      return this
    },
    addListener(event, fn) {
      return this.on(event, fn)
    },
    pause() {
      return this
    },
    resume() {
      return this
    },
    isPaused() {
      return false
    },
    unpipe() {
      return this
    },
    unshift() {},
    read() {
      return null
    },
    setTimeout(ms, cb) {
      if (typeof cb === "function") this.on("timeout", cb)
      return this
    },
    destroy(error) {
      if (error) queueMicrotask(() => this.emit("error", error))
      closeMessage(this)
      return this
    },
    pipe(dest) {
      queueMicrotask(() => {
        if (messageState(this).bytes.length !== 0) dest.write(messageChunk(this))
        queueMicrotask(() => dest.end())
      })
      return dest
    },
    text() {
      return Promise.resolve(decoder.decode(messageState(this).bytes))
    },
    json() {
      return this.text().then((t) => JSON.parse(t))
    },
    arrayBuffer() {
      return Promise.resolve(messageState(this).bytes.slice().buffer)
    },
    async *[Symbol.asyncIterator]() {
      if (messageState(this).bytes.length !== 0) yield messageChunk(this)
    },
  })
  const readableText = async (msg) => {
    const parts = []
    for await (const chunk of msg) parts.push(typeof chunk === "string" ? encoder.encode(chunk) : chunk)
    const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0))
    let at = 0
    for (const p of parts) {
      out.set(p, at)
      at += p.length
    }
    return out
  }
  for (const key of Reflect.ownKeys(IncomingMessage.prototype)) {
    if (key === "constructor") continue
    const custom = Object.getOwnPropertyDescriptor(IncomingMessage.prototype, key).value
    if (typeof custom !== "function") continue
    const viaStream = key === "text" ? (m) => readableText(m).then((b) => decoder.decode(b)) : key === "json" ? (m) => readableText(m).then((b) => JSON.parse(decoder.decode(b))) : key === "arrayBuffer" ? (m) => readableText(m).then((b) => b.buffer) : null
    defineMethods(IncomingMessage.prototype, {
      [key]: function (...args) {
        if (this[kReadableMode]) {
          if (viaStream) return viaStream(this)
          const base = Object.getPrototypeOf(IncomingMessage.prototype)?.[key]
          if (typeof base === "function") return base.apply(this, args)
        }
        return custom.apply(this, args)
      },
    })
  }

  const kResponse = Symbol("responseState")
  const responseState = (res) => {
    let state = res[kResponse]
    if (!state) {
      state = { headers: new Headers(), chunks: [], done: null, controller: null, scheduled: false }
      Object.defineProperty(res, kResponse, { value: state, configurable: true })
    }
    return state
  }
  const responseBytes = (c) => (typeof c === "string" ? encoder.encode(c) : c instanceof Uint8Array ? c : c instanceof ArrayBuffer ? new Uint8Array(c) : ArrayBuffer.isView(c) ? viewBytes(c) : encoder.encode(String(c)))
  const startStreaming = (res) => {
    const state = responseState(res)
    if (state.controller || !state.done || res.writableEnded || res.destroyed) return
    sendHeaders(res)
    let controller
    const stream = new ReadableStream({
      start(c) {
        controller = c
      },
    })
    state.controller = controller
    for (const c of state.chunks) controller.enqueue(responseBytes(c))
    state.chunks = []
    state.done(res.statusCode, wireHeaders(state), stream)
  }
  const wireHeaders = (state) => state.headers[kWireList]()
  const rememberName = (state, name) => {
    const key = String(name).toLowerCase()
    state.names ??= new Map()
    state.order ??= []
    if (!state.names.has(key)) state.order.push(key)
    state.names.set(key, String(name))
  }
  const socketMode = (res) => !responseState(res).done && res.socket != null && typeof res.socket.write === "function" && !(res.socket instanceof NullSocket)
  const socketHead = (res) => {
    const state = responseState(res)
    if (state.socketHead) return
    state.socketHead = true
    const { headers } = state
    const status = res.statusCode
    const bodyless = res.req?.method === "HEAD" || status === 204 || status === 304 || (status >= 100 && status < 200)
    const req = res.req
    const legacy = req != null && (req.httpVersionMajor < 1 || req.httpVersionMinor < 1)
    const chunkedDefault = legacy ? /(?:^|\W)chunked(?:$|\W)/i.test(req.headers?.te ?? "") : true
    state.chunked = chunkedDefault && !bodyless && !headers.has("content-length") && !headers.has("transfer-encoding")
    let text = `HTTP/1.1 ${status} ${res.statusMessage || STATUS_CODES[status] || ""}\r\n`
    const order = state.order ?? []
    const listed = new Set(order)
    for (const key of [...order, ...[...headers.keys()].filter((k) => !listed.has(k))]) {
      if (!headers.has(key)) continue
      const name = state.names?.get(key) ?? key
      const values = key === "set-cookie" ? headers.getSetCookie() : [headers.get(key)]
      for (const value of values) text += `${name}: ${value}\r\n`
    }
    if (!headers.has("date") && res.sendDate !== false && !state.noDate) text += `Date: ${httpDate()}\r\n`
    if (!headers.has("connection") && !state.noConnection) text += "Connection: keep-alive\r\n"
    if (state.chunked) text += "Transfer-Encoding: chunked\r\n"
    text += "\r\n"
    res._header = text
    res._headerSent = true
    res.headersSent = true
    res.socket.write(text)
  }
  const socketWrite = (res, chunk, encoding) => {
    socketHead(res)
    if (chunk == null) return
    const bytes = typeof chunk === "string" ? (typeof Buffer === "function" ? Buffer.from(chunk, typeof encoding === "string" ? encoding : "utf8") : encoder.encode(chunk)) : chunk
    if (!bytes.length) return
    if (responseState(res).chunked) {
      res.socket.write(bytes.length.toString(16) + "\r\n")
      res.socket.write(bytes)
      res.socket.write("\r\n")
    } else res.socket.write(bytes)
  }
  const abortResponse = (res) => {
    const state = responseState(res)
    if (res.writableFinished || res.destroyed) return
    res.destroyed = true
    if (state.controller) {
      try {
        state.controller.close()
      } catch {}
    }
    queueMicrotask(() => res.emit("close"))
    if (res.req) closeMessage(res.req)
  }
  function ServerResponse(req) {
    const state = responseState(this)
    if (typeof req === "function") state.done = req
    else if (req && typeof req === "object") this.req = req
    this.statusCode = 200
    this.statusMessage = ""
    this.headersSent = false
    this.writableEnded = false
    this.writableFinished = false
    this.finished = false
    this.destroyed = false
    this.sendDate = true
    this.socket = null
    this.connection = null
  }
  let dateSecond = -1
  let dateText = ""
  const httpDate = () => {
    const second = Math.floor(Date.now() / 1000)
    if (second !== dateSecond) {
      dateSecond = second
      dateText = new Date(second * 1000).toUTCString()
    }
    return dateText
  }
  const sendHeaders = (res) => {
    if (res.headersSent && typeof res._header === "string") return
    const { headers } = responseState(res)
    let text = `HTTP/1.1 ${res.statusCode} ${res.statusMessage || STATUS_CODES[res.statusCode] || ""}\r\n`
    for (const [name, value] of headers) text += `${name}: ${value}\r\n`
    if (!headers.has("date") && res.sendDate !== false) text += `Date: ${httpDate()}\r\n`
    if (!headers.has("connection")) text += "Connection: keep-alive\r\n"
    text += "\r\n"
    res._header = text
    res._headerSent = true
    res.headersSent = true
  }
  defineMethods(ServerResponse.prototype, messageEmitter)
  defineMethods(ServerResponse.prototype, {
    flushHeaders() {
      sendHeaders(this)
      startStreaming(this)
    },
    cork() {},
    uncork() {},
    setTimeout(ms, cb) {
      if (typeof cb === "function") this.on("timeout", cb)
      return this
    },
    status(code) {
      this.statusCode = code
      return this
    },
    assignSocket(socket) {
      this.socket = socket
      this.connection = socket
      return this
    },
    detachSocket(socket) {
      if (this.socket === socket) {
        this.socket = null
        this.connection = null
      }
      return this
    },
    setHeader(name, value) {
      const state = responseState(this)
      rememberName(state, name)
      if (Array.isArray(value)) {
        state.headers.delete(name)
        for (const v of value) state.headers.append(name, v)
      } else {
        state.headers.set(name, value)
      }
      return this
    },
    appendHeader(name, value) {
      const state = responseState(this)
      rememberName(state, name)
      for (const v of Array.isArray(value) ? value : [value]) state.headers.append(name, v)
      return this
    },
    getHeader(name) {
      return responseState(this).headers.get(name) ?? undefined
    },
    getHeaders() {
      return Object.fromEntries(responseState(this).headers)
    },
    getHeaderNames() {
      return [...responseState(this).headers.keys()]
    },
    hasHeader(name) {
      return responseState(this).headers.has(name)
    },
    removeHeader(name) {
      const state = responseState(this)
      const key = String(name).toLowerCase()
      if (key === "date") state.noDate = true
      if (key === "connection") state.noConnection = true
      state.headers.delete(name)
      state.names?.delete(key)
      if (state.order) state.order = state.order.filter((k) => k !== key)
    },
    writeHead(status, message, headers) {
      if (typeof message === "object" && message !== null) [message, headers] = [undefined, message]
      this.statusCode = status
      if (message !== undefined) this.statusMessage = message
      if (Array.isArray(headers)) {
        if (headers.length && Array.isArray(headers[0])) for (const [k, v] of headers) this.appendHeader(k, v)
        else for (let i = 0; i + 1 < headers.length; i += 2) this.appendHeader(headers[i], headers[i + 1])
      } else if (headers) for (const k of Object.keys(headers)) this.setHeader(k, headers[k])
      responseState(this).explicitHead = true
      sendHeaders(this)
      return this
    },
    _implicitHeader() {
      this.writeHead(this.statusCode)
    },
    writeContinue() {},
    writeEarlyHints() {},
    addTrailers() {},
    write(chunk, encoding, cb) {
      if (typeof encoding === "function") [encoding, cb] = [undefined, encoding]
      const state = responseState(this)
      if (this.destroyed) {
        if (typeof cb === "function") queueMicrotask(cb)
        return false
      }
      if (socketMode(this)) {
        socketWrite(this, chunk, encoding)
        if (typeof cb === "function") queueMicrotask(cb)
        return true
      }
      if (chunk != null) {
        const encoded = typeof chunk === "string" && typeof encoding === "string" && !/^utf-?8$/i.test(encoding) && typeof Buffer === "function" ? Buffer.from(chunk, encoding) : chunk
        if (state.controller) state.controller.enqueue(responseBytes(encoded))
        else state.chunks.push(encoded)
      }
      sendHeaders(this)
      if (!state.controller && !state.scheduled && state.done && !this.writableEnded) {
        state.scheduled = true
        queueMicrotask(() => {
          state.scheduled = false
          startStreaming(this)
        })
      }
      if (typeof cb === "function") queueMicrotask(cb)
      return true
    },
    end(chunk, encoding, cb) {
      if (typeof chunk === "function") [chunk, encoding, cb] = [undefined, undefined, chunk]
      else if (typeof encoding === "function") [encoding, cb] = [undefined, encoding]
      if (this.writableEnded) return this
      const state = responseState(this)
      if (socketMode(this)) {
        if (chunk != null) socketWrite(this, chunk, encoding)
        socketHead(this)
        if (state.chunked) this.socket.write("0\r\n\r\n")
        this.writableEnded = true
        this.finished = true
        if (typeof cb === "function") queueMicrotask(cb)
        queueMicrotask(() => {
          this.writableFinished = true
          this.emit("finish")
          this.emit("close")
        })
        return this
      }
      if (state.controller) {
        if (chunk != null) this.write(chunk, encoding)
        this.writableEnded = true
        this.finished = true
        try {
          state.controller.close()
        } catch {}
        if (typeof cb === "function") queueMicrotask(cb)
        queueMicrotask(() => {
          this.writableFinished = true
          this.emit("finish")
          this.emit("close")
          if (this.req) closeMessage(this.req)
        })
        return this
      }
      if (chunk != null) this.write(chunk, encoding)
      this.writableEnded = true
      this.finished = true
      sendHeaders(this)
      const textual = state.chunks.every((c) => typeof c !== "object")
      let body
      if (textual) {
        body = state.chunks.join("")
      } else {
        const parts = state.chunks.map((c) => (typeof c === "object" ? new Uint8Array(c.buffer ?? c, c.byteOffset ?? 0, c.byteLength) : encoder.encode(String(c))))
        body = new Uint8Array(parts.reduce((n, p) => n + p.length, 0))
        let at = 0
        for (const p of parts) {
          body.set(p, at)
          at += p.length
        }
      }
      if (state.done) state.done(this.statusCode, wireHeaders(state), body, state.explicitHead === true && !state.headers.has("content-length"))
      if (typeof cb === "function") queueMicrotask(cb)
      queueMicrotask(() => {
        this.writableFinished = true
        this.emit("finish")
        this.emit("close")
        if (this.req) closeMessage(this.req)
      })
      return this
    },
    destroy(error) {
      if (this.destroyed) return this
      this.destroyed = true
      if (error) queueMicrotask(() => this.emit("error", error))
      if (!this.writableEnded) queueMicrotask(() => this.emit("close"))
      return this
    },
  })

  const SECURE = Symbol("secureServer")
  function createSecureServer(options, listener) {
    if (typeof options === "function") [options, listener] = [{}, options]
    const opts = { ...(options ?? {}) }
    const ctx = opts.secureContext ?? {}
    opts.key ??= ctx.key
    opts.cert ??= ctx.cert
    if (opts.pfx) {
      const e = new Error("PFX/PKCS#12 certificates are not supported in this runtime; pass PEM key and cert")
      e.code = "ERR_FEATURE_UNAVAILABLE_ON_PLATFORM"
      throw e
    }
    opts[SECURE] = true
    return createServer(opts, listener)
  }
  function createServer(options, listener) {
    if (typeof options === "function") [options, listener] = [{}, options]
    const secure = options?.[SECURE] === true
    const tls = secure ? { key: options.key, cert: options.cert, requestCert: options.requestCert, ca: options.ca, rejectUnauthorized: options.rejectUnauthorized } : null
    let srv = null
    const handlers = { request: listener ? [listener] : [], listening: [], close: [] }
    const onceWrappers = new WeakMap()
    const emit = (name, ...args) => {
      let any = false
      for (const h of (handlers[name] ?? []).slice()) {
        any = true
        if (onceWrappers.has(h)) server.off(name, h)
        h(...args)
      }
      return any
    }
    const server = {
      listening: false,
      get _handle() {
        return srv ? { fd: -1 } : null
      },
      timeout: 0,
      keepAliveTimeout: 5000,
      headersTimeout: 60000,
      requestTimeout: 300000,
      maxHeadersCount: null,
      on(event, fn) {
        ;(handlers[event] ??= []).push(fn)
        return server
      },
      once(event, fn) {
        const wrapper = (...args) => fn(...args)
        onceWrappers.set(wrapper, fn)
        ;(handlers[event] ??= []).push(wrapper)
        return server
      },
      off(event, fn) {
        const list = handlers[event]
        if (list) handlers[event] = list.filter((h) => h !== fn && onceWrappers.get(h) !== fn)
        return server
      },
      removeListener(event, fn) {
        return server.off(event, fn)
      },
      addListener(event, fn) {
        return server.on(event, fn)
      },
      prependListener(event, fn) {
        ;(handlers[event] ??= []).unshift(fn)
        return server
      },
      removeAllListeners(event) {
        if (event === undefined) for (const k of Object.keys(handlers)) handlers[k] = []
        else handlers[event] = []
        return server
      },
      emit,
      listenerCount(event) {
        return handlers[event]?.length ?? 0
      },
      listeners(event) {
        return (handlers[event] ?? []).map((h) => onceWrappers.get(h) ?? h)
      },
      rawListeners(event) {
        return [...(handlers[event] ?? [])]
      },
      eventNames() {
        return Object.keys(handlers).filter((k) => handlers[k].length > 0)
      },
      emit(event, ...args) {
        return emit(event, ...args)
      },
      prependOnceListener(event, fn) {
        const wrapper = (...args) => fn(...args)
        onceWrappers.set(wrapper, fn)
        ;(handlers[event] ??= []).unshift(wrapper)
        return server
      },
      setMaxListeners() {
        return server
      },
      getMaxListeners() {
        return 10
      },
      setTimeout(ms, cb) {
        server.timeout = ms
        if (typeof cb === "function") server.on("timeout", cb)
        return server
      },
      ref() {
        return server
      },
      unref() {
        return server
      },
      closeAllConnections() {},
      closeIdleConnections() {},
      getConnections(cb) {
        if (typeof cb === "function") queueMicrotask(() => cb(null, 0))
      },
      listen(...args) {
        const cb = typeof args[args.length - 1] === "function" ? args.pop() : undefined
        let port = 0
        let host
        let path
        if (typeof args[0] === "object" && args[0] !== null) {
          port = args[0].port ?? 0
          host = args[0].host
          path = args[0].path ?? undefined
        } else if (typeof args[0] === "string" && !(Number(args[0]) >= 0)) {
          path = args[0]
        } else {
          if (args[0] != null) port = args[0]
          if (typeof args[1] === "string") host = args[1]
        }
        if (typeof port === "string" && /^\d+$/.test(port)) port = Number(port)
        const onUpgrade = (raw, id, localAddress, localPort, remoteAddress, remotePort, head) => {
          const socket = require("node:net")[Symbol.for("jsq.socketFromId")]({ id, localAddress, localPort, remoteAddress, remotePort })
          const event = raw.method === "CONNECT" ? "connect" : "upgrade"
          if ((handlers[event]?.length ?? 0) === 0) {
            socket.destroy()
            return
          }
          const req = new IncomingMessage({ [kRawRequest]: raw, [kRaw]: null })
          req.socket = socket
          req.connection = socket
          emit(event, req, socket, Buffer.from(head.buffer, head.byteOffset, head.byteLength))
        }
        try {
          if (secure && (tls.key == null || tls.cert == null)) {
            const e = new Error("An HTTPS server needs a PEM key and cert")
            e.code = "ERR_MISSING_ARGS"
            throw e
          }
          const closers = new Map()
          const onRequest = (raw, replyId) => {
            if (raw === null) {
              closers.get(replyId)?.()
              return
            }
            let replied = false
            const reply = (status, headers, body, chunked) => {
              if (replied) return
              replied = true
              closers.delete(replyId)
              sendReply(replyId, status, headers, body, chunked)
            }
            const hasBody = raw.body.length > 0 && raw.method !== "GET" && raw.method !== "HEAD"
            const message = new IncomingMessage({ [kRawRequest]: raw, [kRaw]: hasBody ? raw.body : null })
            const response = new ServerResponse(reply)
            response.req = message
            closers.set(replyId, () => {
              closers.delete(replyId)
              abortResponse(response)
            })
            try {
              emit("request", message, response)
            } catch (err) {
              console.error(err)
              reply(500, [["content-type", "text/plain;charset=UTF-8"]], "Internal Server Error")
            }
          }
          srv = path !== undefined ? nativeListenPath(String(path), onUpgrade, onRequest) : nativeListen({ port, hostname: host ?? "::", upgrade: onUpgrade, tls }, onRequest)
        } catch (e) {
          if ((handlers.error?.length ?? 0) === 0) throw e
          queueMicrotask(() => emit("error", e))
          return server
        }
        server.listening = true
        if (typeof cb === "function") server.once("listening", cb)
        queueMicrotask(() => emit("listening"))
        return server
      },
      close(cb) {
        const running = srv !== null
        if (srv) srv.stop()
        srv = null
        server.listening = false
        if (typeof cb === "function") {
          if (running) server.once("close", cb)
          else
            server.once("close", () => {
              const e = new Error("Server is not running.")
              e.code = "ERR_SERVER_NOT_RUNNING"
              cb(e)
            })
        }
        queueMicrotask(() => emit("close"))
        return server
      },
      address() {
        if (!srv) return null
        if (srv.path !== undefined) return srv.path
        return { address: srv.hostname, family: srv.hostname.includes(":") ? "IPv6" : "IPv4", port: srv.port }
      },
    }
    Object.setPrototypeOf(server, Server.prototype)
    return server
  }
  function Server(options, listener) {
    return createServer(options, listener)
  }

  const STATUS_CODES = native.statusCodes

  for (const [C, tag] of [
    [URL, "URL"],
    [URLSearchParams, "URLSearchParams"],
    [Headers, "Headers"],
    [Request, "Request"],
    [Response, "Response"],
    [AbortController, "AbortController"],
    [AbortSignal, "AbortSignal"],
    [Event, "Event"],
    [CustomEvent, "CustomEvent"],
    [MessageEvent, "MessageEvent"],
    [EventTarget, "EventTarget"],
    [MessagePort, "MessagePort"],
    [MessageChannel, "MessageChannel"],
    [DOMException, "DOMException"],
  ]) {
    if (!Object.prototype.hasOwnProperty.call(C.prototype, Symbol.toStringTag)) {
      Object.defineProperty(C.prototype, Symbol.toStringTag, { value: tag, configurable: true })
    }
  }

  const inspectCustom = Symbol.for("nodejs.util.inspect.custom")
  const quoteInspect = (v) => {
    const s = String(v)
    if (!s.includes("'")) return `'${s}'`
    if (!s.includes('"')) return `"${s}"`
    return `\`${s}\``
  }
  const defineInspect = (proto, fn) => Object.defineProperty(proto, inspectCustom, { value: fn, writable: true, configurable: true })
  defineInspect(URLSearchParams.prototype, function () {
    const items = [...this].map(([k, v]) => `${quoteInspect(k)} => ${quoteInspect(v)}`)
    return items.length === 0 ? "URLSearchParams {}" : `URLSearchParams { ${items.join(", ")} }`
  })
  defineInspect(URL.prototype, function () {
    const q = quoteInspect
    return `URL {\n  href: ${q(this.href)},\n  origin: ${q(this.origin)},\n  protocol: ${q(this.protocol)},\n  username: ${q(this.username)},\n  password: ${q(this.password)},\n  host: ${q(this.host)},\n  hostname: ${q(this.hostname)},\n  port: ${q(this.port)},\n  pathname: ${q(this.pathname)},\n  search: ${q(this.search)},\n  searchParams: ${this.searchParams[inspectCustom]()},\n  hash: ${q(this.hash)}\n}`
  })
  const viewInspect = (name, props) => function (depth, options, inspect) {
    const p = props(this)
    if (typeof inspect === "function") return `${name} ${inspect(p, options)}`
    return Object.assign(new ({ [name]: class {} })[name](), p)
  }
  defineInspect(ReadableStream.prototype, viewInspect("ReadableStream", (s) => ({
    locked: s.locked,
    state: s[kStream]?.state ?? "readable",
    supportsBYOB: s[kStream]?.type === "bytes",
  })))
  defineInspect(AbortSignal.prototype, viewInspect("AbortSignal", (s) => ({ aborted: s.aborted })))
  for (const C of [Response, Request]) {
    for (const k of ["arrayBuffer", "blob", "body", "bodyUsed", "bytes", "formData", "json", "text"]) {
      const d = Object.getOwnPropertyDescriptor(Body.prototype, k)
      if (d) Object.defineProperty(C.prototype, k, d)
    }
  }
  defineInspect(Response.prototype, viewInspect("Response", (r) => ({
    status: r.status,
    statusText: r.statusText,
    headers: r.headers,
    body: r.body,
    bodyUsed: r.bodyUsed,
    ok: r.ok,
    redirected: r.redirected,
    type: r.type,
    url: r.url,
  })))
  defineInspect(Request.prototype, viewInspect("Request", (r) => ({
    method: r.method,
    url: r.url,
    headers: r.headers,
    destination: r.destination,
    referrer: r.referrer,
    referrerPolicy: r.referrerPolicy,
    mode: r.mode,
    credentials: r.credentials,
    cache: r.cache,
    redirect: r.redirect,
    integrity: r.integrity,
    keepalive: r.keepalive,
    isReloadNavigation: r.isReloadNavigation,
    isHistoryNavigation: r.isHistoryNavigation,
    signal: r.signal,
  })))
  defineInspect(Headers.prototype, function () {
    const items = this[kHeaderList]().map(([k, v]) => `${/^[a-z_$][\w$]*$/i.test(k) ? k : quoteInspect(k)}: ${quoteInspect(v)}`)
    return items.length === 0 ? "Headers {}" : `Headers { ${items.join(", ")} }`
  })

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
    MessageEvent,
    CloseEvent,
    ErrorEvent,
    MessageChannel,
    MessagePort,
    BroadcastChannel,
    EventTarget,
    DOMException,
    Blob,
    File,
    FormData,
    ReadableStream,
    WritableStream,
    TransformStream,
    ReadableStreamDefaultReader,
    ReadableStreamBYOBReader,
    ReadableStreamDefaultController,
    ReadableByteStreamController,
    ReadableStreamBYOBRequest,
    WritableStreamDefaultWriter,
    WritableStreamDefaultController,
    TransformStreamDefaultController,
    TextEncoderStream,
    TextDecoderStream,
    CompressionStream,
    DecompressionStream,
    ByteLengthQueuingStrategy,
    CountQueuingStrategy,
    fetch,
    serve,
    createServer,
    createSecureServer,
    Server,
    IncomingMessage,
    ServerResponse,
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
    method(vm, &natives, "fetch", 8, native_fetch);
    method(vm, &natives, "fetchBody", 3, native_fetch_body);
    method(vm, &natives, "serve", 3, native_serve);
    method(vm, &natives, "servePath", 3, native_serve_path);
    method(vm, &natives, "reply", 4, native_reply);
    method(vm, &natives, "replyHead", 3, native_reply_head);
    method(vm, &natives, "replyChunk", 2, native_reply_chunk);
    method(vm, &natives, "replyEnd", 2, native_reply_end);
    method(vm, &natives, "close", 1, native_close);
    method(vm, &natives, "abort", 1, native_abort);
    method(vm, &natives, "unref", 1, native_unref);
    super::threads::install_natives(vm, &natives);
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
        "Server",
        "IncomingMessage",
        "ServerResponse",
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
    o.set_own("maxHeaderSize", Value::Number(16384.0));
    o.set_hidden("default", Value::Object(o.clone()));
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

pub struct FetchBody {
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handlers: Option<(Value, Value)>,
    queued: Vec<Result<Vec<u8>, Option<String>>>,
}

fn fetch_error_code(msg: &str) -> Option<(&'static str, &'static str, i32)> {
    let (code, syscall, [windows, linux, apple]) =
        if msg.contains("refused") || msg.contains("10061") {
            ("ECONNREFUSED", "connect", [-4078, -111, -61])
        } else if msg.contains("reset") || msg.contains("10054") {
            ("ECONNRESET", "read", [-4077, -104, -54])
        } else if msg.contains("timed out") || msg.contains("timeout") {
            ("ETIMEDOUT", "connect", [-4039, -110, -60])
        } else if msg.contains("host not found")
            || msg.contains("dns")
            || msg.contains("no such host")
            || msg.contains("name or service not known")
            || msg.contains("nodename nor servname")
        {
            ("ENOTFOUND", "getaddrinfo", [-3008, -3008, -3008])
        } else if msg.contains("unreachable") {
            ("EHOSTUNREACH", "connect", [-4065, -113, -65])
        } else {
            return None;
        };
    let errno = if cfg!(windows) {
        windows
    } else if cfg!(target_vendor = "apple") {
        apple
    } else {
        linux
    };
    Some((code, syscall, errno))
}

fn fetch_body_deliver(
    vm: &mut Vm,
    id: u64,
    piece: Result<Vec<u8>, Option<String>>,
) -> Result<(), Value> {
    let Some(entry) = vm.io_fetch_bodies.get_mut(&id) else {
        return Ok(());
    };
    let Some((on_data, on_end)) = entry.handlers.clone() else {
        entry.queued.push(piece);
        return Ok(());
    };
    match piece {
        Ok(bytes) => {
            let chunk = super::binary::new_uint8(vm, bytes);
            vm.call_value(&on_data, Value::Undefined, &[chunk])?;
        }
        Err(error) => {
            vm.io_fetch_bodies.remove(&id);
            vm.io.pending -= 1;
            let arg = error.map(Value::from).unwrap_or(Value::Undefined);
            vm.call_value(&on_end, Value::Undefined, &[arg])?;
        }
    }
    Ok(())
}

fn native_fetch_body(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    let Some(entry) = vm.io_fetch_bodies.get_mut(&id) else {
        return Ok(Value::Bool(false));
    };
    entry.handlers = Some((arg(args, 1), arg(args, 2)));
    let queued = std::mem::take(&mut entry.queued);
    for piece in queued {
        fetch_body_deliver(vm, id, piece)?;
    }
    Ok(Value::Bool(true))
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
    let tls = super::netio::tls_client_options(vm, &arg(args, 6), String::new())?;
    let decode = !matches!(arg(args, 7), Value::Bool(false));
    let p = vm.new_promise();
    let id = vm.io.next_id();
    vm.io.pending += 1;
    vm.io_fetches.insert(id, p.clone());
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    vm.io_fetch_bodies.insert(
        id,
        FetchBody {
            cancel: cancel.clone(),
            handlers: None,
            queued: Vec::new(),
        },
    );
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
            tls,
            cancel,
            decode,
        },
    );
    Ok(pair)
}

fn native_abort(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    vm.io_fetches.remove(&id);
    if let Some(entry) = vm.io_fetch_bodies.remove(&id) {
        entry
            .cancel
            .store(true, std::sync::atomic::Ordering::SeqCst);
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
    let upgrade = arg(args, 3);
    let alpn = vm.new_array(vec!["http/1.1".into()]);
    let tls = super::netio::server_tls(
        vm,
        &arg(args, 4),
        &arg(args, 5),
        &alpn,
        &arg(args, 6),
        &arg(args, 7),
    )?;
    let id = vm.io.next_id();
    let handle = match io::listen(
        &host,
        port as u16,
        id,
        vm.io.ids.clone(),
        vm.io.tx.clone(),
        tls,
    ) {
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
    let frame = vm.async_frame.clone();
    vm.io_servers.insert(id, (dispatch, upgrade, handle, frame));
    Ok(vm.new_array(vec![
        (id as f64).into(),
        (addr.port() as f64).into(),
        addr.ip().to_string().into(),
    ]))
}

fn native_serve_path(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let path = vm.to_string(&arg(args, 0))?.to_string();
    let dispatch = arg(args, 1);
    let upgrade = arg(args, 2);
    let id = vm.io.next_id();
    let handle = match io::listen_path(&path, id, vm.io.ids.clone(), vm.io.tx.clone()) {
        Ok(h) => h,
        Err(e) => {
            let code = io::error_code(&e);
            let err = vm.make_error(ErrorKind::Error, format!("listen {code}: {path}"));
            if let Value::Object(o) = &err {
                o.set_own("code", code.into());
                o.set_own("syscall", "listen".into());
                o.set_own("address", path.clone().into());
            }
            return Err(err);
        }
    };
    vm.io.pending += 1;
    let frame = vm.async_frame.clone();
    vm.io_servers.insert(id, (dispatch, upgrade, handle, frame));
    Ok(Value::Number(id as f64))
}

fn native_reply(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    let status = vm.to_number(&arg(args, 1))? as u16;
    let headers = pairs(vm, &arg(args, 2))?;
    let body = match super::binary::buffer_source_bytes(&arg(args, 3)) {
        Some(b) => b,
        None => vm.to_string(&arg(args, 3))?.as_bytes().to_vec(),
    };
    let chunked = arg(args, 4).truthy();
    if let Some(tx) = vm.io_replies.remove(&id) {
        let _ = tx.send(ReplyMsg::Full(HttpReply {
            status,
            headers,
            body,
            chunked,
        }));
    }
    Ok(Value::Undefined)
}

fn native_reply_head(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    let status = vm.to_number(&arg(args, 1))? as u16;
    let headers = pairs(vm, &arg(args, 2))?;
    if let Some(tx) = vm.io_replies.get(&id) {
        let _ = tx.send(ReplyMsg::Head { status, headers });
    }
    Ok(Value::Undefined)
}

fn native_reply_chunk(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    let body = match super::binary::buffer_source_bytes(&arg(args, 1)) {
        Some(b) => b,
        None => vm.to_string(&arg(args, 1))?.as_bytes().to_vec(),
    };
    let sent = match vm.io_replies.get(&id) {
        Some(tx) => tx.send(ReplyMsg::Chunk(body)).is_ok(),
        None => false,
    };
    Ok(Value::Bool(sent))
}

fn native_reply_end(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    let abort = arg(args, 1).truthy();
    if let Some(tx) = vm.io_replies.remove(&id) {
        let _ = tx.send(if abort {
            ReplyMsg::Abort
        } else {
            ReplyMsg::End
        });
    }
    Ok(Value::Undefined)
}

fn native_close(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    if let Some((_, _, handle, _)) = vm.io_servers.remove(&id) {
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
    Value::Object(o)
}

fn request_value(vm: &mut Vm, r: HttpRequest) -> Value {
    let o = vm.new_object();
    o.set_own("method", r.method.into());
    o.set_own("url", r.url.into());
    o.set_own("target", r.target.into());
    let headers = pairs_value(vm, r.headers);
    o.set_own("headers", headers);
    o.set_own("body", super::binary::new_uint8(vm, r.body));
    o.set_own(
        "remote",
        if r.remote.is_empty() {
            Value::Undefined
        } else {
            r.remote.into()
        },
    );
    if let Some(cert) = r.peer_cert {
        let cert = super::binary::new_uint8(vm, cert);
        o.set_own("peerCert", cert);
    }
    Value::Object(o)
}

pub fn handle_io(vm: &mut Vm, ev: IoEvent) -> Result<(), Value> {
    match ev {
        IoEvent::Stdin(chunk) => super::node::handle_stdin(vm, chunk)?,
        IoEvent::Socket { id, event } => super::netio::handle_socket(vm, id, event)?,
        IoEvent::Accept {
            server,
            id,
            local,
            remote,
            cmd,
            alpn,
            peer,
        } => super::netio::handle_accept(vm, server, id, local, remote, cmd, alpn, peer)?,
        IoEvent::Child { id, event } => super::netio::handle_child(vm, id, event)?,
        IoEvent::Watch { id, event } => super::netio::handle_watch(vm, id, event)?,
        IoEvent::Port { port, event } => super::threads::handle_port(vm, port, event)?,
        IoEvent::Broadcast { id, data } => super::threads::handle_broadcast(vm, id, data)?,
        IoEvent::Worker { id, event } => super::threads::handle_worker(vm, id, event)?,
        IoEvent::Wake => {}
        IoEvent::WaitDone { id, result } => super::threads::handle_wait_done(vm, id, result),
        IoEvent::FetchChunk { id, data } => fetch_body_deliver(vm, id, Ok(data))?,
        IoEvent::Napi(ev) => crate::napi::handle_event(vm, ev)?,
        IoEvent::FetchEnd { id, error } => fetch_body_deliver(vm, id, Err(error))?,
        IoEvent::Fetch { id, result } => {
            let Some(p) = vm.io_fetches.remove(&id) else {
                return Ok(());
            };
            match result {
                Ok(r) => {
                    let v = fetch_value(vm, r);
                    vm.resolve_promise(&p, v);
                }
                Err(msg) => {
                    if vm.io_fetch_bodies.remove(&id).is_some() {
                        vm.io.pending -= 1;
                    }
                    let e = vm.type_error("fetch failed");
                    let lower = msg.to_ascii_lowercase();
                    let code = fetch_error_code(&lower);
                    let cause = vm.make_error(ErrorKind::Error, msg);
                    if let (Value::Object(c), Some((code, syscall, errno))) = (&cause, code) {
                        c.set_own("code", code.into());
                        c.set_own("syscall", syscall.into());
                        if errno != 0 {
                            c.set_own("errno", Value::Number(errno as f64));
                        }
                    }
                    if let Value::Object(o) = &e {
                        o.set_own("cause", cause);
                    }
                    vm.reject_promise(&p, e);
                }
            }
        }
        IoEvent::Request {
            server,
            rid,
            request,
            reply,
        } => {
            let Some((dispatch, _, _, frame)) = vm.io_servers.get(&server) else {
                return Ok(());
            };
            let dispatch = dispatch.clone();
            let frame = frame.clone();
            vm.io_replies.insert(rid, reply);
            let raw = request_value(vm, request);
            let prev = std::mem::replace(&mut vm.async_frame, frame);
            let r = vm.call_value(&dispatch, Value::Undefined, &[raw, (rid as f64).into()]);
            vm.async_frame = prev;
            r?;
        }
        IoEvent::RequestClosed { server, rid } => {
            if !vm.io_replies.contains_key(&rid) {
                return Ok(());
            }
            let Some((dispatch, _, _, frame)) = vm.io_servers.get(&server) else {
                return Ok(());
            };
            let dispatch = dispatch.clone();
            let prev = std::mem::replace(&mut vm.async_frame, frame.clone());
            let r = vm.call_value(
                &dispatch,
                Value::Undefined,
                &[Value::Null, (rid as f64).into()],
            );
            vm.async_frame = prev;
            r?;
        }
        IoEvent::Upgrade {
            server,
            id,
            request,
            local,
            remote,
            cmd,
            head,
        } => {
            let (upgrade, frame) = match vm.io_servers.get(&server) {
                Some((_, upgrade, _, frame)) if upgrade.is_callable() => {
                    (upgrade.clone(), frame.clone())
                }
                _ => {
                    let _ = cmd.send(crate::io::SocketCmd::Destroy);
                    return Ok(());
                }
            };
            vm.io_sockets.insert(
                id,
                super::netio::SocketEntry {
                    cmd,
                    handler: Value::Undefined,
                    refed: true,
                    frame: frame.clone(),
                },
            );
            vm.io.pending += 1;
            let raw = request_value(vm, request);
            let head = super::binary::new_uint8(vm, head);
            let prev = std::mem::replace(&mut vm.async_frame, frame);
            let r = vm.call_value(
                &upgrade,
                Value::Undefined,
                &[
                    raw,
                    Value::Number(id as f64),
                    local.map_or(Value::Undefined, |a| a.ip().to_string().into()),
                    local.map_or(Value::Undefined, |a| Value::Number(a.port() as f64)),
                    remote.map_or(Value::Undefined, |a| a.ip().to_string().into()),
                    remote.map_or(Value::Undefined, |a| Value::Number(a.port() as f64)),
                    head,
                ],
            );
            vm.async_frame = prev;
            r?;
        }
    }
    Ok(())
}
