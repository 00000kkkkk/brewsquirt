(function () {
  const constants = {"NGHTTP2_ERR_FRAME_SIZE_ERROR":-522,"NGHTTP2_SESSION_SERVER":0,"NGHTTP2_SESSION_CLIENT":1,"NGHTTP2_STREAM_STATE_IDLE":1,"NGHTTP2_STREAM_STATE_OPEN":2,"NGHTTP2_STREAM_STATE_RESERVED_LOCAL":3,"NGHTTP2_STREAM_STATE_RESERVED_REMOTE":4,"NGHTTP2_STREAM_STATE_HALF_CLOSED_LOCAL":5,"NGHTTP2_STREAM_STATE_HALF_CLOSED_REMOTE":6,"NGHTTP2_STREAM_STATE_CLOSED":7,"NGHTTP2_FLAG_NONE":0,"NGHTTP2_FLAG_END_STREAM":1,"NGHTTP2_FLAG_END_HEADERS":4,"NGHTTP2_FLAG_ACK":1,"NGHTTP2_FLAG_PADDED":8,"NGHTTP2_FLAG_PRIORITY":32,"DEFAULT_SETTINGS_HEADER_TABLE_SIZE":4096,"DEFAULT_SETTINGS_ENABLE_PUSH":1,"DEFAULT_SETTINGS_MAX_CONCURRENT_STREAMS":4294967295,"DEFAULT_SETTINGS_INITIAL_WINDOW_SIZE":65535,"DEFAULT_SETTINGS_MAX_FRAME_SIZE":16384,"DEFAULT_SETTINGS_MAX_HEADER_LIST_SIZE":65535,"DEFAULT_SETTINGS_ENABLE_CONNECT_PROTOCOL":0,"MAX_MAX_FRAME_SIZE":16777215,"MIN_MAX_FRAME_SIZE":16384,"MAX_INITIAL_WINDOW_SIZE":2147483647,"NGHTTP2_SETTINGS_HEADER_TABLE_SIZE":1,"NGHTTP2_SETTINGS_ENABLE_PUSH":2,"NGHTTP2_SETTINGS_MAX_CONCURRENT_STREAMS":3,"NGHTTP2_SETTINGS_INITIAL_WINDOW_SIZE":4,"NGHTTP2_SETTINGS_MAX_FRAME_SIZE":5,"NGHTTP2_SETTINGS_MAX_HEADER_LIST_SIZE":6,"NGHTTP2_SETTINGS_ENABLE_CONNECT_PROTOCOL":8,"PADDING_STRATEGY_NONE":0,"PADDING_STRATEGY_ALIGNED":1,"PADDING_STRATEGY_MAX":2,"PADDING_STRATEGY_CALLBACK":1,"NGHTTP2_NO_ERROR":0,"NGHTTP2_PROTOCOL_ERROR":1,"NGHTTP2_INTERNAL_ERROR":2,"NGHTTP2_FLOW_CONTROL_ERROR":3,"NGHTTP2_SETTINGS_TIMEOUT":4,"NGHTTP2_STREAM_CLOSED":5,"NGHTTP2_FRAME_SIZE_ERROR":6,"NGHTTP2_REFUSED_STREAM":7,"NGHTTP2_CANCEL":8,"NGHTTP2_COMPRESSION_ERROR":9,"NGHTTP2_CONNECT_ERROR":10,"NGHTTP2_ENHANCE_YOUR_CALM":11,"NGHTTP2_INADEQUATE_SECURITY":12,"NGHTTP2_HTTP_1_1_REQUIRED":13,"NGHTTP2_DEFAULT_WEIGHT":16,"HTTP2_HEADER_STATUS":":status","HTTP2_HEADER_METHOD":":method","HTTP2_HEADER_AUTHORITY":":authority","HTTP2_HEADER_SCHEME":":scheme","HTTP2_HEADER_PATH":":path","HTTP2_HEADER_PROTOCOL":":protocol","HTTP2_HEADER_ACCEPT_ENCODING":"accept-encoding","HTTP2_HEADER_ACCEPT_LANGUAGE":"accept-language","HTTP2_HEADER_ACCEPT_RANGES":"accept-ranges","HTTP2_HEADER_ACCEPT":"accept","HTTP2_HEADER_ACCESS_CONTROL_ALLOW_CREDENTIALS":"access-control-allow-credentials","HTTP2_HEADER_ACCESS_CONTROL_ALLOW_HEADERS":"access-control-allow-headers","HTTP2_HEADER_ACCESS_CONTROL_ALLOW_METHODS":"access-control-allow-methods","HTTP2_HEADER_ACCESS_CONTROL_ALLOW_ORIGIN":"access-control-allow-origin","HTTP2_HEADER_ACCESS_CONTROL_EXPOSE_HEADERS":"access-control-expose-headers","HTTP2_HEADER_ACCESS_CONTROL_REQUEST_HEADERS":"access-control-request-headers","HTTP2_HEADER_ACCESS_CONTROL_REQUEST_METHOD":"access-control-request-method","HTTP2_HEADER_AGE":"age","HTTP2_HEADER_AUTHORIZATION":"authorization","HTTP2_HEADER_CACHE_CONTROL":"cache-control","HTTP2_HEADER_CONNECTION":"connection","HTTP2_HEADER_CONTENT_DISPOSITION":"content-disposition","HTTP2_HEADER_CONTENT_ENCODING":"content-encoding","HTTP2_HEADER_CONTENT_LENGTH":"content-length","HTTP2_HEADER_CONTENT_TYPE":"content-type","HTTP2_HEADER_COOKIE":"cookie","HTTP2_HEADER_DATE":"date","HTTP2_HEADER_ETAG":"etag","HTTP2_HEADER_FORWARDED":"forwarded","HTTP2_HEADER_HOST":"host","HTTP2_HEADER_IF_MODIFIED_SINCE":"if-modified-since","HTTP2_HEADER_IF_NONE_MATCH":"if-none-match","HTTP2_HEADER_IF_RANGE":"if-range","HTTP2_HEADER_LAST_MODIFIED":"last-modified","HTTP2_HEADER_LINK":"link","HTTP2_HEADER_LOCATION":"location","HTTP2_HEADER_RANGE":"range","HTTP2_HEADER_REFERER":"referer","HTTP2_HEADER_SERVER":"server","HTTP2_HEADER_SET_COOKIE":"set-cookie","HTTP2_HEADER_STRICT_TRANSPORT_SECURITY":"strict-transport-security","HTTP2_HEADER_TRANSFER_ENCODING":"transfer-encoding","HTTP2_HEADER_TE":"te","HTTP2_HEADER_UPGRADE_INSECURE_REQUESTS":"upgrade-insecure-requests","HTTP2_HEADER_UPGRADE":"upgrade","HTTP2_HEADER_USER_AGENT":"user-agent","HTTP2_HEADER_VARY":"vary","HTTP2_HEADER_X_CONTENT_TYPE_OPTIONS":"x-content-type-options","HTTP2_HEADER_X_FRAME_OPTIONS":"x-frame-options","HTTP2_HEADER_KEEP_ALIVE":"keep-alive","HTTP2_HEADER_PROXY_CONNECTION":"proxy-connection","HTTP2_HEADER_X_XSS_PROTECTION":"x-xss-protection","HTTP2_HEADER_ALT_SVC":"alt-svc","HTTP2_HEADER_CONTENT_SECURITY_POLICY":"content-security-policy","HTTP2_HEADER_EARLY_DATA":"early-data","HTTP2_HEADER_EXPECT_CT":"expect-ct","HTTP2_HEADER_ORIGIN":"origin","HTTP2_HEADER_PURPOSE":"purpose","HTTP2_HEADER_TIMING_ALLOW_ORIGIN":"timing-allow-origin","HTTP2_HEADER_X_FORWARDED_FOR":"x-forwarded-for","HTTP2_HEADER_PRIORITY":"priority","HTTP2_HEADER_ACCEPT_CHARSET":"accept-charset","HTTP2_HEADER_ACCESS_CONTROL_MAX_AGE":"access-control-max-age","HTTP2_HEADER_ALLOW":"allow","HTTP2_HEADER_CONTENT_LANGUAGE":"content-language","HTTP2_HEADER_CONTENT_LOCATION":"content-location","HTTP2_HEADER_CONTENT_MD5":"content-md5","HTTP2_HEADER_CONTENT_RANGE":"content-range","HTTP2_HEADER_DNT":"dnt","HTTP2_HEADER_EXPECT":"expect","HTTP2_HEADER_EXPIRES":"expires","HTTP2_HEADER_FROM":"from","HTTP2_HEADER_IF_MATCH":"if-match","HTTP2_HEADER_IF_UNMODIFIED_SINCE":"if-unmodified-since","HTTP2_HEADER_MAX_FORWARDS":"max-forwards","HTTP2_HEADER_PREFER":"prefer","HTTP2_HEADER_PROXY_AUTHENTICATE":"proxy-authenticate","HTTP2_HEADER_PROXY_AUTHORIZATION":"proxy-authorization","HTTP2_HEADER_REFRESH":"refresh","HTTP2_HEADER_RETRY_AFTER":"retry-after","HTTP2_HEADER_TRAILER":"trailer","HTTP2_HEADER_TK":"tk","HTTP2_HEADER_VIA":"via","HTTP2_HEADER_WARNING":"warning","HTTP2_HEADER_WWW_AUTHENTICATE":"www-authenticate","HTTP2_HEADER_HTTP2_SETTINGS":"http2-settings","HTTP2_METHOD_ACL":"ACL","HTTP2_METHOD_BASELINE_CONTROL":"BASELINE-CONTROL","HTTP2_METHOD_BIND":"BIND","HTTP2_METHOD_CHECKIN":"CHECKIN","HTTP2_METHOD_CHECKOUT":"CHECKOUT","HTTP2_METHOD_CONNECT":"CONNECT","HTTP2_METHOD_COPY":"COPY","HTTP2_METHOD_DELETE":"DELETE","HTTP2_METHOD_GET":"GET","HTTP2_METHOD_HEAD":"HEAD","HTTP2_METHOD_LABEL":"LABEL","HTTP2_METHOD_LINK":"LINK","HTTP2_METHOD_LOCK":"LOCK","HTTP2_METHOD_MERGE":"MERGE","HTTP2_METHOD_MKACTIVITY":"MKACTIVITY","HTTP2_METHOD_MKCALENDAR":"MKCALENDAR","HTTP2_METHOD_MKCOL":"MKCOL","HTTP2_METHOD_MKREDIRECTREF":"MKREDIRECTREF","HTTP2_METHOD_MKWORKSPACE":"MKWORKSPACE","HTTP2_METHOD_MOVE":"MOVE","HTTP2_METHOD_OPTIONS":"OPTIONS","HTTP2_METHOD_ORDERPATCH":"ORDERPATCH","HTTP2_METHOD_PATCH":"PATCH","HTTP2_METHOD_POST":"POST","HTTP2_METHOD_PRI":"PRI","HTTP2_METHOD_PROPFIND":"PROPFIND","HTTP2_METHOD_PROPPATCH":"PROPPATCH","HTTP2_METHOD_PUT":"PUT","HTTP2_METHOD_REBIND":"REBIND","HTTP2_METHOD_REPORT":"REPORT","HTTP2_METHOD_SEARCH":"SEARCH","HTTP2_METHOD_TRACE":"TRACE","HTTP2_METHOD_UNBIND":"UNBIND","HTTP2_METHOD_UNCHECKOUT":"UNCHECKOUT","HTTP2_METHOD_UNLINK":"UNLINK","HTTP2_METHOD_UNLOCK":"UNLOCK","HTTP2_METHOD_UPDATE":"UPDATE","HTTP2_METHOD_UPDATEREDIRECTREF":"UPDATEREDIRECTREF","HTTP2_METHOD_VERSION_CONTROL":"VERSION-CONTROL","HTTP_STATUS_CONTINUE":100,"HTTP_STATUS_SWITCHING_PROTOCOLS":101,"HTTP_STATUS_PROCESSING":102,"HTTP_STATUS_EARLY_HINTS":103,"HTTP_STATUS_OK":200,"HTTP_STATUS_CREATED":201,"HTTP_STATUS_ACCEPTED":202,"HTTP_STATUS_NON_AUTHORITATIVE_INFORMATION":203,"HTTP_STATUS_NO_CONTENT":204,"HTTP_STATUS_RESET_CONTENT":205,"HTTP_STATUS_PARTIAL_CONTENT":206,"HTTP_STATUS_MULTI_STATUS":207,"HTTP_STATUS_ALREADY_REPORTED":208,"HTTP_STATUS_IM_USED":226,"HTTP_STATUS_MULTIPLE_CHOICES":300,"HTTP_STATUS_MOVED_PERMANENTLY":301,"HTTP_STATUS_FOUND":302,"HTTP_STATUS_SEE_OTHER":303,"HTTP_STATUS_NOT_MODIFIED":304,"HTTP_STATUS_USE_PROXY":305,"HTTP_STATUS_TEMPORARY_REDIRECT":307,"HTTP_STATUS_PERMANENT_REDIRECT":308,"HTTP_STATUS_BAD_REQUEST":400,"HTTP_STATUS_UNAUTHORIZED":401,"HTTP_STATUS_PAYMENT_REQUIRED":402,"HTTP_STATUS_FORBIDDEN":403,"HTTP_STATUS_NOT_FOUND":404,"HTTP_STATUS_METHOD_NOT_ALLOWED":405,"HTTP_STATUS_NOT_ACCEPTABLE":406,"HTTP_STATUS_PROXY_AUTHENTICATION_REQUIRED":407,"HTTP_STATUS_REQUEST_TIMEOUT":408,"HTTP_STATUS_CONFLICT":409,"HTTP_STATUS_GONE":410,"HTTP_STATUS_LENGTH_REQUIRED":411,"HTTP_STATUS_PRECONDITION_FAILED":412,"HTTP_STATUS_PAYLOAD_TOO_LARGE":413,"HTTP_STATUS_URI_TOO_LONG":414,"HTTP_STATUS_UNSUPPORTED_MEDIA_TYPE":415,"HTTP_STATUS_RANGE_NOT_SATISFIABLE":416,"HTTP_STATUS_EXPECTATION_FAILED":417,"HTTP_STATUS_TEAPOT":418,"HTTP_STATUS_MISDIRECTED_REQUEST":421,"HTTP_STATUS_UNPROCESSABLE_ENTITY":422,"HTTP_STATUS_LOCKED":423,"HTTP_STATUS_FAILED_DEPENDENCY":424,"HTTP_STATUS_TOO_EARLY":425,"HTTP_STATUS_UPGRADE_REQUIRED":426,"HTTP_STATUS_PRECONDITION_REQUIRED":428,"HTTP_STATUS_TOO_MANY_REQUESTS":429,"HTTP_STATUS_REQUEST_HEADER_FIELDS_TOO_LARGE":431,"HTTP_STATUS_UNAVAILABLE_FOR_LEGAL_REASONS":451,"HTTP_STATUS_INTERNAL_SERVER_ERROR":500,"HTTP_STATUS_NOT_IMPLEMENTED":501,"HTTP_STATUS_BAD_GATEWAY":502,"HTTP_STATUS_SERVICE_UNAVAILABLE":503,"HTTP_STATUS_GATEWAY_TIMEOUT":504,"HTTP_STATUS_HTTP_VERSION_NOT_SUPPORTED":505,"HTTP_STATUS_VARIANT_ALSO_NEGOTIATES":506,"HTTP_STATUS_INSUFFICIENT_STORAGE":507,"HTTP_STATUS_LOOP_DETECTED":508,"HTTP_STATUS_BANDWIDTH_LIMIT_EXCEEDED":509,"HTTP_STATUS_NOT_EXTENDED":510,"HTTP_STATUS_NETWORK_AUTHENTICATION_REQUIRED":511}
  const HUFFMAN_LENGTHS = [13,23,28,28,28,28,28,28,28,24,30,28,28,30,28,28,28,28,28,28,28,28,30,28,28,28,28,28,28,28,28,28,6,10,10,12,13,6,8,11,10,10,8,11,8,6,6,6,5,5,5,6,6,6,6,6,6,6,7,8,15,6,12,10,13,6,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,8,7,8,13,19,13,14,6,15,5,6,5,6,5,6,6,6,5,7,7,6,6,6,5,6,7,6,5,5,6,7,7,7,7,7,15,11,14,13,28,20,22,20,20,22,22,22,23,22,23,23,23,23,23,24,23,24,24,22,23,24,23,23,23,23,21,22,23,22,23,23,24,22,21,20,22,22,23,23,21,23,22,22,24,21,22,23,23,21,21,22,21,23,22,23,23,20,22,22,22,23,22,22,23,26,26,20,19,22,23,22,25,26,26,26,27,27,26,24,25,19,21,26,27,27,26,27,24,21,21,26,26,28,27,27,27,20,24,20,21,22,21,21,23,22,22,25,25,24,24,26,23,26,27,26,26,27,27,27,27,27,28,27,27,27,27,27,26,30]
  const HUFFMAN_CODES = [8184,8388568,268435426,268435427,268435428,268435429,268435430,268435431,268435432,16777194,1073741820,268435433,268435434,1073741821,268435435,268435436,268435437,268435438,268435439,268435440,268435441,268435442,1073741822,268435443,268435444,268435445,268435446,268435447,268435448,268435449,268435450,268435451,20,1016,1017,4090,8185,21,248,2042,1018,1019,249,2043,250,22,23,24,0,1,2,25,26,27,28,29,30,31,92,251,32764,32,4091,1020,8186,33,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,252,115,253,8187,524272,8188,16380,34,32765,3,35,4,36,5,37,38,39,6,116,117,40,41,42,7,43,118,44,8,9,45,119,120,121,122,123,32766,2044,16381,8189,268435452,1048550,4194258,1048551,1048552,4194259,4194260,4194261,8388569,4194262,8388570,8388571,8388572,8388573,8388574,16777195,8388575,16777196,16777197,4194263,8388576,16777198,8388577,8388578,8388579,8388580,2097116,4194264,8388581,4194265,8388582,8388583,16777199,4194266,2097117,1048553,4194267,4194268,8388584,8388585,2097118,8388586,4194269,4194270,16777200,2097119,4194271,8388587,8388588,2097120,2097121,4194272,2097122,8388589,4194273,8388590,8388591,1048554,4194274,4194275,4194276,8388592,4194277,4194278,8388593,67108832,67108833,1048555,524273,4194279,8388594,4194280,33554412,67108834,67108835,67108836,134217694,134217695,67108837,16777201,33554413,524274,2097123,67108838,134217696,134217697,67108839,134217698,16777202,2097124,2097125,67108840,67108841,268435453,134217699,134217700,134217701,1048556,16777203,1048557,2097126,4194281,2097127,2097128,8388595,4194282,4194283,33554414,33554415,16777204,16777205,67108842,8388596,67108843,134217702,67108844,67108845,134217703,134217704,134217705,134217706,134217707,268435454,134217708,134217709,134217710,134217711,134217712,67108846,1073741823]
  const STATIC_TABLE = [[":authority",""],[":method","GET"],[":method","POST"],[":path","/"],[":path","/index.html"],[":scheme","http"],[":scheme","https"],[":status","200"],[":status","204"],[":status","206"],[":status","304"],[":status","400"],[":status","404"],[":status","500"],["accept-charset",""],["accept-encoding","gzip, deflate"],["accept-language",""],["accept-ranges",""],["accept",""],["access-control-allow-origin",""],["age",""],["allow",""],["authorization",""],["cache-control",""],["content-disposition",""],["content-encoding",""],["content-language",""],["content-length",""],["content-location",""],["content-range",""],["content-type",""],["cookie",""],["date",""],["etag",""],["expect",""],["expires",""],["from",""],["host",""],["if-match",""],["if-modified-since",""],["if-none-match",""],["if-range",""],["if-unmodified-since",""],["last-modified",""],["link",""],["location",""],["max-forwards",""],["proxy-authenticate",""],["proxy-authorization",""],["range",""],["referer",""],["refresh",""],["retry-after",""],["server",""],["set-cookie",""],["strict-transport-security",""],["transfer-encoding",""],["user-agent",""],["vary",""],["via",""],["www-authenticate",""]]
  const EventEmitter = process.getBuiltinModule("node:events")
  const net = process.getBuiltinModule("node:net")
  const tls = process.getBuiltinModule("node:tls")
  const fs = process.getBuiltinModule("node:fs")
  const { Duplex, Readable, Stream } = process.getBuiltinModule("node:stream")

  const PREFACE = Buffer.from("PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n", "latin1")
  const DATA = 0
  const HEADERS = 1
  const RST_STREAM = 3
  const SETTINGS = 4
  const PUSH_PROMISE = 5
  const PING = 6
  const GOAWAY = 7
  const WINDOW_UPDATE = 8
  const CONTINUATION = 9
  const FLAG_END_STREAM = 1
  const FLAG_ACK = 1
  const FLAG_END_HEADERS = 4
  const FLAG_PADDED = 8
  const FLAG_PRIORITY = 32
  const SETTING_IDS = { headerTableSize: 1, enablePush: 2, maxConcurrentStreams: 3, initialWindowSize: 4, maxFrameSize: 5, maxHeaderListSize: 6, enableConnectProtocol: 8 }
  const SETTING_NAMES = Object.fromEntries(Object.entries(SETTING_IDS).map(([k, v]) => [v, k]))
  const defaultSettings = () => ({ headerTableSize: 4096, enablePush: true, initialWindowSize: 65535, maxFrameSize: 16384, maxConcurrentStreams: 4294967295, maxHeaderSize: 65535, maxHeaderListSize: 65535, enableConnectProtocol: false })
  const ERROR_NAMES = ["NGHTTP2_NO_ERROR", "NGHTTP2_PROTOCOL_ERROR", "NGHTTP2_INTERNAL_ERROR", "NGHTTP2_FLOW_CONTROL_ERROR", "NGHTTP2_SETTINGS_TIMEOUT", "NGHTTP2_STREAM_CLOSED", "NGHTTP2_FRAME_SIZE_ERROR", "NGHTTP2_REFUSED_STREAM", "NGHTTP2_CANCEL", "NGHTTP2_COMPRESSION_ERROR", "NGHTTP2_CONNECT_ERROR", "NGHTTP2_ENHANCE_YOUR_CALM", "NGHTTP2_INADEQUATE_SECURITY", "NGHTTP2_HTTP_1_1_REQUIRED"]
  const errorName = (code) => ERROR_NAMES[code] ?? String(code)
  const codedError = (code, message, Base = Error) => {
    const e = new Base(message)
    e.code = code
    return e
  }
  const sensitiveHeaders = Symbol("sensitiveHeaders")
  const kSession = Symbol("session")
  const kState = Symbol("state")
  const kStream = Symbol("stream")
  const kHeaders = Symbol("headers")
  const kTrailers = Symbol("trailers")
  const kOptions = Symbol("options")
  const CONNECTION_HEADERS = new Set(["connection", "keep-alive", "proxy-connection", "transfer-encoding", "upgrade", "http2-settings"])
  const SINGLE_VALUE = new Set([":status", ":method", ":authority", ":scheme", ":path", ":protocol", "access-control-allow-credentials", "access-control-max-age", "access-control-request-method", "age", "authorization", "content-encoding", "content-language", "content-length", "content-location", "content-md5", "content-range", "content-type", "date", "dnt", "etag", "expires", "from", "host", "if-match", "if-modified-since", "if-none-match", "if-range", "if-unmodified-since", "last-modified", "location", "max-forwards", "proxy-authorization", "range", "referer", "retry-after", "tk", "upgrade-insecure-requests", "user-agent", "x-content-type-options"])

  let huffmanTree = null
  const buildHuffmanTree = () => {
    const root = [null, null]
    for (let sym = 0; sym < 257; sym++) {
      const len = HUFFMAN_LENGTHS[sym]
      const code = HUFFMAN_CODES[sym]
      let node = root
      for (let i = len - 1; i >= 0; i--) {
        const bit = (code >>> i) & 1
        if (i === 0) node[bit] = sym
        else {
          if (!Array.isArray(node[bit])) node[bit] = [null, null]
          node = node[bit]
        }
      }
    }
    return root
  }
  const compressionError = () => codedError("ERR_HTTP2_COMPRESSION", "HPACK decoding failed")
  const huffmanDecode = (bytes) => {
    huffmanTree ??= buildHuffmanTree()
    const out = []
    let node = huffmanTree
    let pending = 0
    let ones = true
    for (let k = 0; k < bytes.length; k++) {
      const byte = bytes[k]
      for (let i = 7; i >= 0; i--) {
        const bit = (byte >> i) & 1
        const next = node[bit]
        if (next === null || next === undefined) throw compressionError()
        if (typeof next === "number") {
          if (next === 256) throw compressionError()
          out.push(next)
          node = huffmanTree
          pending = 0
          ones = true
        } else {
          node = next
          pending++
          if (!bit) ones = false
        }
      }
    }
    if (pending > 7 || !ones) throw compressionError()
    return Buffer.from(out).toString("latin1")
  }
  const decodeInt = (buf, pos, prefix) => {
    const max = (1 << prefix) - 1
    let value = buf[pos] & max
    pos++
    if (value < max) return [value, pos]
    let shift = 0
    for (;;) {
      if (pos >= buf.length || shift > 28) throw compressionError()
      const b = buf[pos++]
      value += (b & 127) * 2 ** shift
      shift += 7
      if (!(b & 128)) break
    }
    return [value, pos]
  }
  const encodeInt = (out, value, prefix, first) => {
    const max = (1 << prefix) - 1
    if (value < max) {
      out.push(first | value)
      return
    }
    out.push(first | max)
    value -= max
    while (value >= 128) {
      out.push((value % 128) + 128)
      value = Math.floor(value / 128)
    }
    out.push(value)
  }
  class HpackDecoder {
    constructor(size) {
      this.dynamic = []
      this.size = 0
      this.maxSize = size
      this.limit = size
    }
    entry(index) {
      if (index <= 0) throw compressionError()
      if (index <= STATIC_TABLE.length) return STATIC_TABLE[index - 1]
      const e = this.dynamic[index - STATIC_TABLE.length - 1]
      if (!e) throw compressionError()
      return e
    }
    evict() {
      while (this.size > this.maxSize && this.dynamic.length) {
        const [n, v] = this.dynamic.pop()
        this.size -= n.length + v.length + 32
      }
    }
    insert(name, value) {
      this.dynamic.unshift([name, value])
      this.size += name.length + value.length + 32
      this.evict()
    }
    readString(buf, pos) {
      if (pos >= buf.length) throw compressionError()
      const huffman = buf[pos] & 128
      let len
      ;[len, pos] = decodeInt(buf, pos, 7)
      if (pos + len > buf.length) throw compressionError()
      const raw = buf.subarray(pos, pos + len)
      return [huffman ? huffmanDecode(raw) : raw.toString("latin1"), pos + len]
    }
    literal(buf, pos, prefix) {
      let index
      ;[index, pos] = decodeInt(buf, pos, prefix)
      let name
      if (index === 0) [name, pos] = this.readString(buf, pos)
      else name = this.entry(index)[0]
      let value
      ;[value, pos] = this.readString(buf, pos)
      return [name, value, pos]
    }
    decode(buf) {
      const out = []
      let pos = 0
      while (pos < buf.length) {
        const b = buf[pos]
        if (b & 128) {
          let index
          ;[index, pos] = decodeInt(buf, pos, 7)
          const [n, v] = this.entry(index)
          out.push([n, v, false])
        } else if (b & 64) {
          let n, v
          ;[n, v, pos] = this.literal(buf, pos, 6)
          this.insert(n, v)
          out.push([n, v, false])
        } else if (b & 32) {
          let size
          ;[size, pos] = decodeInt(buf, pos, 5)
          if (size > this.limit) throw compressionError()
          this.maxSize = size
          this.evict()
        } else {
          let n, v
          ;[n, v, pos] = this.literal(buf, pos, 4)
          out.push([n, v, (b & 16) !== 0])
        }
      }
      return out
    }
  }
  const staticFull = new Map()
  const staticName = new Map()
  STATIC_TABLE.forEach(([n, v], i) => {
    if (!staticFull.has(n + "\0" + v)) staticFull.set(n + "\0" + v, i + 1)
    if (!staticName.has(n)) staticName.set(n, i + 1)
  })
  const pushString = (out, s) => {
    const bytes = Buffer.from(s, "latin1")
    encodeInt(out, bytes.length, 7, 0)
    for (let i = 0; i < bytes.length; i++) out.push(bytes[i])
  }
  const encodeHeaderBlock = (pairs, sensitive) => {
    const out = []
    for (const [name, value] of pairs) {
      const never = sensitive.has(name)
      const full = never ? undefined : staticFull.get(name + "\0" + value)
      if (full) {
        encodeInt(out, full, 7, 128)
        continue
      }
      const index = staticName.get(name) ?? 0
      encodeInt(out, index, 4, never ? 16 : 0)
      if (!index) pushString(out, name)
      pushString(out, value)
    }
    return Buffer.from(out)
  }

  const headerPairs = (headers, defaults) => {
    const pseudo = []
    const regular = []
    const seen = new Set()
    const sensitive = new Set((headers?.[sensitiveHeaders] ?? []).map((n) => String(n).toLowerCase()))
    const add = (name, value) => {
      const key = String(name).toLowerCase()
      if (value === undefined || value === null) return
      if (CONNECTION_HEADERS.has(key) || (key === "te" && String(value).toLowerCase() !== "trailers")) {
        throw codedError("ERR_HTTP2_INVALID_CONNECTION_HEADERS", `HTTP/1 Connection specific headers are forbidden: "${key}"`, TypeError)
      }
      const list = Array.isArray(value) ? value : [value]
      if (key[0] === ":") {
        if (seen.has(key)) throw codedError("ERR_HTTP2_HEADER_SINGLE_VALUE", `Header field "${key}" must only have a single value`, TypeError)
        seen.add(key)
        pseudo.push([key, String(list[0])])
        return
      }
      seen.add(key)
      for (const v of list) regular.push([key, String(v)])
    }
    for (const [k, v] of Object.entries(headers ?? {})) add(k, v)
    for (const [k, v] of Object.entries(defaults ?? {})) if (!seen.has(k)) add(k, v)
    return [[...pseudo, ...regular], sensitive]
  }
  const headerObject = (list) => {
    const headers = Object.create(null)
    const raw = []
    const sensitive = []
    for (const [name, value, never] of list) {
      raw.push(name, value)
      if (never) sensitive.push(name)
      const existing = headers[name]
      if (existing === undefined) headers[name] = name === "set-cookie" ? [value] : value
      else if (!SINGLE_VALUE.has(name)) {
        if (name === "cookie") headers[name] = existing + "; " + value
        else if (name === "set-cookie") existing.push(value)
        else headers[name] = existing + ", " + value
      }
    }
    if (headers[":status"] !== undefined) headers[":status"] = Number(headers[":status"])
    headers[sensitiveHeaders] = sensitive
    return [headers, raw]
  }

  const packSettings = (settings) => {
    const parts = []
    for (const [key, id] of Object.entries(SETTING_IDS)) {
      let value = settings?.[key]
      if (key === "maxHeaderListSize" && value === undefined) value = settings?.maxHeaderSize
      if (value === undefined) continue
      const b = Buffer.alloc(6)
      b.writeUInt16BE(id, 0)
      b.writeUInt32BE(typeof value === "boolean" ? Number(value) : value >>> 0, 2)
      parts.push(b)
    }
    return Buffer.concat(parts)
  }
  const unpackSettings = (buf) => {
    const out = {}
    for (let i = 0; i + 6 <= buf.length; i += 6) {
      const name = SETTING_NAMES[buf.readUInt16BE(i)]
      if (!name) continue
      const value = buf.readUInt32BE(i + 2)
      out[name] = name === "enablePush" || name === "enableConnectProtocol" ? value !== 0 : value
      if (name === "maxHeaderListSize") out.maxHeaderSize = value
    }
    return out
  }

  const writeFrame = (session, type, flags, id, payload) => {
    const st = session[kState]
    if (st.socketGone) return
    const length = payload ? payload.length : 0
    const header = Buffer.allocUnsafe(9)
    header.writeUIntBE(length, 0, 3)
    header[3] = type
    header[4] = flags
    header.writeUInt32BE(id >>> 0, 5)
    session.socket.write(length ? Buffer.concat([header, payload]) : header)
  }
  const windowUpdate = (session, id, increment) => {
    const b = Buffer.alloc(4)
    b.writeUInt32BE(increment, 0)
    writeFrame(session, WINDOW_UPDATE, 0, id, b)
  }
  const sendHeaderBlock = (session, id, pairs, sensitive, endStream) => {
    const block = encodeHeaderBlock(pairs, sensitive)
    const max = session[kState].remote.maxFrameSize
    let flags = endStream ? FLAG_END_STREAM : 0
    if (block.length <= max) {
      writeFrame(session, HEADERS, flags | FLAG_END_HEADERS, id, block)
      return
    }
    writeFrame(session, HEADERS, flags, id, block.subarray(0, max))
    for (let pos = max; pos < block.length; pos += max) {
      const end = pos + max >= block.length
      writeFrame(session, CONTINUATION, end ? FLAG_END_HEADERS : 0, id, block.subarray(pos, pos + max))
    }
  }

  class Http2Session extends EventEmitter {
    constructor(type, options, socket) {
      super()
      this[kOptions] = options ?? {}
      this.socket = socket
      const local = { ...defaultSettings(), ...(options?.settings ?? {}) }
      this[kState] = {
        type,
        streams: new Map(),
        nextId: type === 1 ? 1 : 2,
        buffer: Buffer.alloc(0),
        prefaceDone: type === 1,
        block: null,
        decoder: new HpackDecoder(local.headerTableSize),
        local,
        remote: defaultSettings(),
        gotRemoteSettings: false,
        sendWindow: 65535,
        recvWindow: 65535,
        pings: [],
        settingsCallbacks: [],
        lastStreamId: 0,
        goawaySent: false,
        closing: false,
        socketGone: false,
        started: false,
      }
      this.closed = false
      this.destroyed = false
      this.connecting = true
      this.pendingSettingsAck = true
      this.originSet = undefined
      socket.on("data", (chunk) => receive(this, chunk))
      socket.on("error", (e) => {
        if (!this.destroyed) this.destroy(e)
      })
      socket.on("close", () => {
        this[kState].socketGone = true
        if (!this.destroyed) this.destroy()
      })
      socket.on("timeout", () => this.emit("timeout"))
      sendPreface(this)
      const start = () => startSession(this)
      if (socket.connecting || socket.pending) socket.once(socket.encrypted ? "secureConnect" : "connect", start)
      else queueMicrotask(start)
    }
    get type() {
      return this[kState].type
    }
    get encrypted() {
      return !!this.socket?.encrypted
    }
    get alpnProtocol() {
      return this.socket?.encrypted ? this.socket.alpnProtocol || "h2" : "h2c"
    }
    get localSettings() {
      return { ...this[kState].local }
    }
    get remoteSettings() {
      return { ...this[kState].remote }
    }
    get state() {
      const st = this[kState]
      return { effectiveLocalWindowSize: st.recvWindow, effectiveRecvDataLength: 0, nextStreamID: st.nextId, localWindowSize: st.recvWindow, lastProcStreamID: st.lastStreamId, remoteWindowSize: st.sendWindow, outboundQueueSize: 1, deflateDynamicTableSize: 0, inflateDynamicTableSize: st.decoder.size }
    }
    settings(settings, cb) {
      const st = this[kState]
      Object.assign(st.pendingLocal ??= {}, settings)
      if (typeof cb === "function") st.settingsCallbacks.push(cb)
      this.pendingSettingsAck = true
      writeFrame(this, SETTINGS, 0, 0, packSettings(settings))
    }
    setLocalWindowSize(size) {
      const st = this[kState]
      if (size > st.recvWindow) windowUpdate(this, 0, size - st.recvWindow)
      st.recvWindow = size
    }
    ping(payload, cb) {
      if (typeof payload === "function") [payload, cb] = [undefined, payload]
      const data = payload ? Buffer.from(payload) : Buffer.alloc(8)
      if (data.length !== 8) throw codedError("ERR_HTTP2_PING_LENGTH", "HTTP2 ping payload must be 8 bytes", RangeError)
      this[kState].pings.push({ data, cb, start: performance.now() })
      writeFrame(this, PING, 0, 0, data)
      return true
    }
    goaway(code = constants.NGHTTP2_NO_ERROR, lastStreamID = this[kState].lastStreamId, opaqueData) {
      const st = this[kState]
      const payload = Buffer.alloc(8 + (opaqueData ? opaqueData.length : 0))
      payload.writeUInt32BE(lastStreamID >>> 0, 0)
      payload.writeUInt32BE(code >>> 0, 4)
      if (opaqueData) Buffer.from(opaqueData).copy(payload, 8)
      st.goawaySent = true
      writeFrame(this, GOAWAY, 0, 0, payload)
    }
    close(cb) {
      if (typeof cb === "function") this.once("close", cb)
      if (this.closed || this.destroyed) return
      this.closed = true
      const st = this[kState]
      if (!st.goawaySent && !st.socketGone) this.goaway(constants.NGHTTP2_NO_ERROR)
      st.closing = true
      maybeFinish(this)
    }
    destroy(error, code) {
      if (this.destroyed) return
      if (typeof error === "number") [error, code] = [undefined, error]
      const st = this[kState]
      this.destroyed = true
      this.closed = true
      code ??= error ? constants.NGHTTP2_INTERNAL_ERROR : constants.NGHTTP2_NO_ERROR
      if (!st.goawaySent && !st.socketGone) {
        try {
          this.goaway(code)
        } catch {}
      }
      for (const stream of [...st.streams.values()]) {
        if (stream.rstCode === undefined) stream.rstCode = error || code !== constants.NGHTTP2_NO_ERROR ? constants.NGHTTP2_INTERNAL_ERROR : constants.NGHTTP2_CANCEL
        stream.destroy(error ? codedError("ERR_HTTP2_STREAM_CANCEL", "The pending stream has been canceled (caused by: " + error.message + ")") : undefined)
      }
      st.streams.clear()
      for (const p of st.pings) queueMicrotask(() => p.cb?.(codedError("ERR_HTTP2_PING_CANCEL", "HTTP2 ping cancelled"), 0, p.data))
      st.pings = []
      if (!st.socketGone) {
        if (error) this.socket.destroy()
        else this.socket.end()
      }
      queueMicrotask(() => {
        if (error) this.emit("error", error)
        this.emit("close")
      })
    }
    ref() {
      this.socket?.ref?.()
    }
    unref() {
      this.socket?.unref?.()
    }
    setTimeout(ms, cb) {
      if (typeof cb === "function") this.once("timeout", cb)
      this.socket?.setTimeout?.(ms)
    }
  }
  const sendPreface = (session) => {
    const st = session[kState]
    const local = st.local
    const initial = {}
    for (const key of Object.keys(SETTING_IDS)) {
      if (session[kOptions].settings && key in session[kOptions].settings) initial[key] = session[kOptions].settings[key]
    }
    if (st.type === 1 && initial.enablePush === undefined) initial.enablePush = false
    if (st.type === 1) session.socket.write(PREFACE)
    writeFrame(session, SETTINGS, 0, 0, packSettings(initial))
    if (local.initialWindowSize > 65535) {
      windowUpdate(session, 0, local.initialWindowSize - 65535)
      st.recvWindow = local.initialWindowSize
    }
  }
  const startSession = (session) => {
    const st = session[kState]
    if (st.started || session.destroyed) return
    st.started = true
    session.connecting = false
    session.emit("connect", session, session.socket)
    if (st.buffer.length) receive(session, Buffer.alloc(0))
  }
  const maybeFinish = (session) => {
    const st = session[kState]
    if (st.closing && st.streams.size === 0 && !session.destroyed) session.destroy()
  }
  const protocolError = (session, code = constants.NGHTTP2_PROTOCOL_ERROR) => {
    session.destroy(codedError("ERR_HTTP2_SESSION_ERROR", `Session closed with error code ${code}`), code)
  }
  const receive = (session, chunk) => {
    const st = session[kState]
    if (session.destroyed) return
    st.buffer = st.buffer.length ? Buffer.concat([st.buffer, chunk]) : chunk
    if (!st.started) return
    if (!st.prefaceDone) {
      if (st.buffer.length < PREFACE.length) {
        if (!PREFACE.subarray(0, st.buffer.length).equals(st.buffer)) protocolError(session)
        return
      }
      if (!st.buffer.subarray(0, PREFACE.length).equals(PREFACE)) return protocolError(session)
      st.buffer = st.buffer.subarray(PREFACE.length)
      st.prefaceDone = true
    }
    while (st.buffer.length >= 9 && !session.destroyed) {
      const length = st.buffer.readUIntBE(0, 3)
      if (length > st.local.maxFrameSize) return protocolError(session, constants.NGHTTP2_FRAME_SIZE_ERROR)
      if (st.buffer.length < 9 + length) break
      const type = st.buffer[3]
      const flags = st.buffer[4]
      const id = st.buffer.readUInt32BE(5) & 0x7fffffff
      const payload = st.buffer.subarray(9, 9 + length)
      st.buffer = st.buffer.subarray(9 + length)
      try {
        onFrame(session, type, flags, id, payload)
      } catch (e) {
        if (e?.code === "ERR_HTTP2_COMPRESSION") return protocolError(session, constants.NGHTTP2_COMPRESSION_ERROR)
        throw e
      }
    }
  }
  const unpad = (flags, payload) => {
    if (!(flags & FLAG_PADDED)) return payload
    const pad = payload[0]
    return payload.subarray(1, payload.length - pad)
  }
  const onFrame = (session, type, flags, id, payload) => {
    const st = session[kState]
    if (st.block && type !== CONTINUATION) return protocolError(session)
    switch (type) {
      case DATA: {
        const stream = st.streams.get(id)
        if (payload.length) {
          windowUpdate(session, 0, payload.length)
          if (stream && !(flags & FLAG_END_STREAM)) windowUpdate(session, id, payload.length)
        }
        if (!stream) return
        const data = unpad(flags, payload)
        if (data.length) stream.push(Buffer.from(data))
        if (flags & FLAG_END_STREAM) remoteEnd(stream)
        return
      }
      case HEADERS: {
        let block = unpad(flags, payload)
        if (flags & FLAG_PRIORITY) block = block.subarray(5)
        st.block = { id, flags, chunks: [block], push: false }
        if (flags & FLAG_END_HEADERS) finishBlock(session)
        return
      }
      case CONTINUATION: {
        if (!st.block || st.block.id !== id) return protocolError(session)
        st.block.chunks.push(payload)
        if (flags & FLAG_END_HEADERS) finishBlock(session)
        return
      }
      case PUSH_PROMISE: {
        const body = unpad(flags, payload)
        const promised = body.readUInt32BE(0) & 0x7fffffff
        st.block = { id, flags, chunks: [body.subarray(4)], push: promised }
        if (flags & FLAG_END_HEADERS) finishBlock(session)
        return
      }
      case RST_STREAM: {
        const stream = st.streams.get(id)
        if (stream) resetByPeer(stream, payload.readUInt32BE(0))
        return
      }
      case SETTINGS: {
        if (flags & FLAG_ACK) {
          session.pendingSettingsAck = false
          if (st.pendingLocal) {
            Object.assign(st.local, st.pendingLocal)
            st.pendingLocal = null
          }
          session.emit("localSettings", session.localSettings)
          const cbs = st.settingsCallbacks.splice(0)
          for (const cb of cbs) cb(null, session.localSettings, 0)
          return
        }
        const changes = unpackSettings(payload)
        if (changes.initialWindowSize !== undefined) {
          const delta = changes.initialWindowSize - st.remote.initialWindowSize
          for (const stream of st.streams.values()) stream[kState].sendWindow += delta
        }
        Object.assign(st.remote, changes)
        writeFrame(session, SETTINGS, FLAG_ACK, 0, null)
        st.gotRemoteSettings = true
        session.emit("remoteSettings", session.remoteSettings)
        flush(session)
        return
      }
      case PING: {
        if (flags & FLAG_ACK) {
          const index = st.pings.findIndex((p) => p.data.equals(payload))
          if (index >= 0) {
            const [p] = st.pings.splice(index, 1)
            p.cb?.(null, performance.now() - p.start, Buffer.from(payload))
          }
          return
        }
        writeFrame(session, PING, FLAG_ACK, 0, Buffer.from(payload))
        session.emit("ping", Buffer.from(payload))
        return
      }
      case GOAWAY: {
        const lastStreamID = payload.readUInt32BE(0) & 0x7fffffff
        const code = payload.readUInt32BE(4)
        const opaque = Buffer.from(payload.subarray(8))
        session.emit("goaway", code, lastStreamID, opaque)
        for (const stream of [...st.streams.values()]) {
          if (st.type === 1 && stream.id > lastStreamID) {
            stream.rstCode = constants.NGHTTP2_REFUSED_STREAM
            stream.destroy(codedError("ERR_HTTP2_STREAM_ERROR", "Stream closed with error code NGHTTP2_REFUSED_STREAM"))
          }
        }
        if (code === constants.NGHTTP2_NO_ERROR) session.close()
        else session.destroy(codedError("ERR_HTTP2_SESSION_ERROR", `Session closed with error code ${code}`), code)
        return
      }
      case WINDOW_UPDATE: {
        const increment = payload.readUInt32BE(0) & 0x7fffffff
        if (id === 0) st.sendWindow += increment
        else {
          const stream = st.streams.get(id)
          if (stream) stream[kState].sendWindow += increment
        }
        flush(session)
        return
      }
      default:
        return
    }
  }
  const finishBlock = (session) => {
    const st = session[kState]
    const { id, flags, chunks, push } = st.block
    st.block = null
    const list = st.decoder.decode(chunks.length === 1 ? chunks[0] : Buffer.concat(chunks))
    if (push !== false) {
      const b = Buffer.alloc(4)
      b.writeUInt32BE(constants.NGHTTP2_REFUSED_STREAM, 0)
      writeFrame(session, RST_STREAM, 0, push, b)
      return
    }
    const [headers, raw] = headerObject(list)
    let stream = st.streams.get(id)
    if (st.type === 0) {
      if (!stream) {
        if (id % 2 === 0 || id <= st.lastStreamId || st.closing) {
          if (id % 2 === 0 || id <= st.lastStreamId) return protocolError(session)
          const b = Buffer.alloc(4)
          b.writeUInt32BE(constants.NGHTTP2_REFUSED_STREAM, 0)
          writeFrame(session, RST_STREAM, 0, id, b)
          return
        }
        st.lastStreamId = id
        stream = new ServerHttp2Stream(session, id, headers)
        st.streams.set(id, stream)
        session.emit("stream", stream, headers, flags, raw)
        if (flags & FLAG_END_STREAM) remoteEnd(stream)
        return
      }
      stream.emit("trailers", headers, flags, raw)
      if (flags & FLAG_END_STREAM) remoteEnd(stream)
      return
    }
    if (!stream) return
    const status = headers[":status"]
    if (!stream[kState].responded && status >= 100 && status < 200) {
      stream.emit("headers", headers, flags, raw)
      if (status === 100) stream.emit("continue")
    } else if (!stream[kState].responded) {
      stream[kState].responded = true
      stream.emit("response", headers, flags, raw)
    } else stream.emit("trailers", headers, flags, raw)
    if (flags & FLAG_END_STREAM) remoteEnd(stream)
  }
  const remoteEnd = (stream) => {
    const st = stream[kState]
    if (st.remoteClosed) return
    st.remoteClosed = true
    stream.push(null)
    maybeClose(stream)
  }
  const streamError = (code) => (code === constants.NGHTTP2_NO_ERROR || code === constants.NGHTTP2_CANCEL ? undefined : codedError("ERR_HTTP2_STREAM_ERROR", `Stream closed with error code ${errorName(code)}`))
  const resetByPeer = (stream, code) => {
    const st = stream[kState]
    stream.rstCode = code
    st.remoteClosed = true
    st.localClosed = true
    const err = streamError(code)
    if (!stream.readableEnded) {
      stream.aborted = true
      stream.emit("aborted")
    }
    finishStream(stream, err)
  }
  const maybeClose = (stream) => {
    const st = stream[kState]
    if (!st.remoteClosed || !st.localClosed) return
    if (stream.rstCode === undefined) stream.rstCode = constants.NGHTTP2_NO_ERROR
    if (stream.readableEnded || stream.destroyed) finishStream(stream)
    else stream.once("end", () => finishStream(stream))
  }
  const finishStream = (stream, err) => {
    const session = stream[kSession]
    if (session) session[kState].streams.delete(stream.id)
    for (const item of stream[kState].queue.splice(0)) item.cb?.(err ?? null)
    if (!stream.destroyed) stream.destroy(err)
    if (session) maybeFinish(session)
  }
  const flush = (session) => {
    const st = session[kState]
    if (session.destroyed) return
    for (const stream of st.streams.values()) {
      const ss = stream[kState]
      while (ss.queue.length && st.sendWindow > 0 && ss.sendWindow > 0) {
        const item = ss.queue[0]
        const size = Math.min(item.data.length - item.offset, st.sendWindow, ss.sendWindow, st.remote.maxFrameSize)
        const piece = item.data.subarray(item.offset, item.offset + size)
        item.offset += size
        st.sendWindow -= size
        ss.sendWindow -= size
        writeFrame(session, DATA, 0, stream.id, piece)
        if (item.offset >= item.data.length) {
          ss.queue.shift()
          item.cb?.()
        }
      }
      if (!ss.queue.length && ss.endPending) {
        ss.endPending = false
        finishWritable(stream)
      }
    }
  }
  const finishWritable = (stream) => {
    const st = stream[kState]
    if (st.waitForTrailers) {
      st.awaitingTrailers = true
      stream.emit("wantTrailers")
      return
    }
    sendEnd(stream)
  }
  const sendEnd = (stream) => {
    const st = stream[kState]
    if (st.localClosed) return
    st.localClosed = true
    writeFrame(stream[kSession], DATA, FLAG_END_STREAM, stream.id, null)
    const cb = st.finalCb
    st.finalCb = null
    cb?.()
    maybeClose(stream)
  }

  class Http2Stream extends Duplex {
    constructor(session, id) {
      super({ allowHalfOpen: true, autoDestroy: false })
      this[kSession] = session
      this.id = id
      this[kState] = { queue: [], sendWindow: session[kState].remote.initialWindowSize, localClosed: false, remoteClosed: false, endPending: false, finalCb: null, waitForTrailers: false, awaitingTrailers: false, responded: false }
      this.rstCode = undefined
      this.aborted = false
      this.headersSent = false
      this.sentHeaders = undefined
      this.sentInfoHeaders = undefined
      this.sentTrailers = undefined
      this.endAfterHeaders = false
      this.pending = false
    }
    get session() {
      return this[kSession]
    }
    get closed() {
      return this.destroyed
    }
    get bufferSize() {
      return this[kState].queue.reduce((n, item) => n + item.data.length - item.offset, 0)
    }
    get state() {
      const st = this[kState]
      return { localWindowSize: st.sendWindow, state: st.localClosed && st.remoteClosed ? 7 : 2, localClose: st.localClosed ? 1 : 0, remoteClose: st.remoteClosed ? 1 : 0, sumDependencyWeight: 0, weight: 16 }
    }
    _read() {}
    _write(chunk, encoding, cb) {
      const st = this[kState]
      if (!this.headersSent && typeof this[kProceed] === "function") this[kProceed]()
      if (st.localClosed) return cb(codedError("ERR_STREAM_WRITE_AFTER_END", "write after end"))
      const data = typeof chunk === "string" ? Buffer.from(chunk, encoding) : Buffer.from(chunk.buffer, chunk.byteOffset, chunk.byteLength)
      st.queue.push({ data, offset: 0, cb })
      flush(this[kSession])
    }
    _final(cb) {
      const st = this[kState]
      if (!this.headersSent && typeof this[kProceed] === "function") this[kProceed]()
      if (st.localClosed) return cb()
      st.finalCb = cb
      st.endPending = true
      flush(this[kSession])
    }
    sendTrailers(headers) {
      const st = this[kState]
      if (!st.awaitingTrailers) throw codedError("ERR_HTTP2_TRAILERS_NOT_READY", "Trailing headers cannot be sent until after the wantTrailers event is emitted")
      st.awaitingTrailers = false
      const [pairs, sensitive] = headerPairs(headers)
      this.sentTrailers = headers
      if (!pairs.length) return sendEnd(this)
      st.localClosed = true
      sendHeaderBlock(this[kSession], this.id, pairs, sensitive, true)
      const cb = st.finalCb
      st.finalCb = null
      cb?.()
      maybeClose(this)
    }
    close(code = constants.NGHTTP2_NO_ERROR, cb) {
      if (typeof cb === "function") this.once("close", cb)
      if (this.destroyed) return
      const session = this[kSession]
      this.rstCode = code
      if (!session.destroyed) {
        const b = Buffer.alloc(4)
        b.writeUInt32BE(code >>> 0, 0)
        writeFrame(session, RST_STREAM, 0, this.id, b)
      }
      const st = this[kState]
      st.localClosed = true
      st.remoteClosed = true
      finishStream(this, streamError(code))
    }
    _destroy(err, cb) {
      const session = this[kSession]
      const st = this[kState]
      if ((!st.localClosed || !st.remoteClosed) && session && !session.destroyed && this.rstCode === undefined) {
        this.rstCode = err ? constants.NGHTTP2_INTERNAL_ERROR : constants.NGHTTP2_CANCEL
        const b = Buffer.alloc(4)
        b.writeUInt32BE(this.rstCode, 0)
        writeFrame(session, RST_STREAM, 0, this.id, b)
      }
      st.localClosed = true
      st.remoteClosed = true
      if (session) {
        session[kState].streams.delete(this.id)
        queueMicrotask(() => maybeFinish(session))
      }
      cb(err)
    }
    priority() {}
    setTimeout(ms, cb) {
      if (typeof cb === "function") this.once("timeout", cb)
      clearTimeout(this[kState].timer)
      if (ms > 0) {
        this[kState].timer = setTimeout(() => this.emit("timeout"), ms)
        this[kState].timer.unref?.()
      }
      return this
    }
  }
  const kProceed = Symbol("proceed")

  class ServerHttp2Stream extends Http2Stream {
    constructor(session, id, headers) {
      super(session, id)
      this[kHeaders] = headers
      this.pushAllowed = false
    }
    [kProceed]() {
      this.respond()
    }
    respond(headers = {}, options = {}) {
      if (this.destroyed) throw codedError("ERR_HTTP2_INVALID_STREAM", "The stream has been destroyed")
      if (this.headersSent) throw codedError("ERR_HTTP2_HEADERS_SENT", "Response has already been initiated.")
      const st = this[kState]
      const [pairs, sensitive] = headerPairs(headers, { ":status": 200 })
      const status = Number(pairs.find((p) => p[0] === ":status")[1])
      if (status < 200 || status > 599) throw codedError("ERR_HTTP2_STATUS_INVALID", `Invalid status code: ${status}`, RangeError)
      if (!pairs.some((p) => p[0] === "date") && options.sendDate !== false) pairs.push(["date", new Date().toUTCString()])
      this.headersSent = true
      this.sentHeaders = headers
      st.waitForTrailers = !!options.waitForTrailers
      const endStream = !!options.endStream || status === 204 || status === 304 || this[kHeaders][":method"] === "HEAD"
      if (endStream && !st.waitForTrailers) {
        st.localClosed = true
        this.endAfterHeaders = true
        sendHeaderBlock(this[kSession], this.id, pairs, sensitive, true)
        queueMicrotask(() => {
          if (!this.writableEnded) this.end()
        })
        maybeClose(this)
        return
      }
      sendHeaderBlock(this[kSession], this.id, pairs, sensitive, false)
      if (endStream) this.end()
    }
    additionalHeaders(headers) {
      const [pairs, sensitive] = headerPairs(headers)
      this.sentInfoHeaders = [...(this.sentInfoHeaders ?? []), headers]
      sendHeaderBlock(this[kSession], this.id, pairs, sensitive, false)
    }
    pushStream(headers, options, cb) {
      if (typeof options === "function") cb = options
      queueMicrotask(() => cb?.(codedError("ERR_HTTP2_PUSH_DISABLED", "HTTP/2 client has disabled push streams")))
    }
    respondWithFile(path, headers = {}, options = {}) {
      fs.readFile(path, (err, data) => {
        if (err) {
          if (typeof options.onError === "function") return options.onError(err)
          return this.destroy(err)
        }
        const start = options.offset ?? 0
        const end = options.length !== undefined ? start + options.length : data.length
        this.respond({ "content-length": String(end - start), ...headers }, { waitForTrailers: options.waitForTrailers })
        this.end(data.subarray(start, end))
      })
    }
    respondWithFD(fd, headers = {}, options = {}) {
      const data = fs.readFileSync(typeof fd === "number" ? fd : fd.fd)
      this.respond({ "content-length": String(data.length), ...headers }, { waitForTrailers: options.waitForTrailers })
      this.end(data)
    }
  }
  class ClientHttp2Stream extends Http2Stream {}

  class ServerHttp2Session extends Http2Session {
    constructor(options, socket, server) {
      super(0, options, socket)
      this.server = server
    }
    altsvc() {}
    origin() {}
  }
  class ClientHttp2Session extends Http2Session {
    constructor(options, socket, url) {
      super(1, options, socket)
      this[kHeaders] = { scheme: url.protocol.replace(":", ""), authority: url.host }
    }
    request(headers = {}, options = {}) {
      if (this.destroyed) throw codedError("ERR_HTTP2_INVALID_SESSION", "The session has been destroyed")
      if (this.closed) throw codedError("ERR_HTTP2_GOAWAY_SESSION", "New streams cannot be created after receiving a GOAWAY")
      const st = this[kState]
      const id = st.nextId
      st.nextId += 2
      const stream = new ClientHttp2Stream(this, id)
      st.streams.set(id, stream)
      const method = String(headers[":method"] ?? "GET").toUpperCase()
      const defaults = method === "CONNECT" ? { ":method": method, ":authority": this[kHeaders].authority } : { ":method": method, ":scheme": this[kHeaders].scheme, ":authority": this[kHeaders].authority, ":path": "/" }
      const [pairs, sensitive] = headerPairs({ ...headers, ":method": method }, defaults)
      const sst = stream[kState]
      sst.waitForTrailers = !!options.waitForTrailers
      stream.headersSent = true
      stream.sentHeaders = headers
      const endStream = options.endStream ?? false
      if (endStream && !sst.waitForTrailers) {
        sst.localClosed = true
        stream.endAfterHeaders = true
      }
      sendHeaderBlock(this, id, pairs, sensitive, endStream && !sst.waitForTrailers)
      if (endStream) queueMicrotask(() => stream.end())
      if (options.signal) {
        const abort = () => stream.destroy(Object.assign(new Error("The operation was aborted"), { name: "AbortError", code: "ABORT_ERR", cause: options.signal.reason }))
        if (options.signal.aborted) queueMicrotask(abort)
        else options.signal.addEventListener("abort", abort, { once: true })
      }
      queueMicrotask(() => stream.emit("ready"))
      return stream
    }
    setLocalWindowSize(size) {
      super.setLocalWindowSize(size)
    }
  }

  const connect = (authority, options, listener) => {
    if (typeof options === "function") [options, listener] = [{}, options]
    options = { ...(options ?? {}) }
    let url
    if (typeof authority === "string" || authority instanceof URL) url = new URL(String(authority))
    else {
      const protocol = authority?.protocol ?? "http:"
      const host = authority?.hostname ?? authority?.host ?? "localhost"
      url = new URL(`${protocol}//${host.includes(":") && !host.startsWith("[") ? `[${host}]` : host}${authority?.port ? ":" + authority.port : ""}`)
    }
    if (url.protocol !== "http:" && url.protocol !== "https:") throw codedError("ERR_HTTP2_UNSUPPORTED_PROTOCOL", `protocol "${url.protocol}" is unsupported.`)
    const host = url.hostname.replace(/^\[|\]$/g, "")
    const port = Number(url.port || (url.protocol === "https:" ? 443 : 80))
    let socket
    if (typeof options.createConnection === "function") socket = options.createConnection(url, options)
    else if (url.protocol === "https:") socket = tls.connect({ ...options, host, port, servername: options.servername ?? (net.isIP(host) ? undefined : host), ALPNProtocols: ["h2"] })
    else socket = net.connect({ host, port })
    const session = new ClientHttp2Session(options, socket, url)
    if (typeof listener === "function") session.once("connect", listener)
    return session
  }

  class Http2ServerRequest extends Readable {
    constructor(stream, headers, options, rawHeaders) {
      super({ autoDestroy: false })
      this[kStream] = stream
      this.headers = headers
      this.rawHeaders = rawHeaders
      this.trailers = {}
      this.rawTrailers = []
      this.complete = false
      this.aborted = false
      stream.on("data", (chunk) => {
        if (!this.push(chunk)) stream.pause()
      })
      stream.on("end", () => {
        this.complete = true
        this.push(null)
      })
      stream.on("trailers", (trailers, flags, raw) => {
        this.trailers = trailers
        this.rawTrailers = raw
      })
      stream.on("aborted", () => {
        this.aborted = true
        this.emit("aborted")
      })
      stream.on("close", () => this.emit("close"))
      stream.on("error", (e) => {
        if (this.listenerCount("error")) this.emit("error", e)
      })
    }
    _read() {
      this[kStream].resume()
    }
    get stream() {
      return this[kStream]
    }
    get method() {
      return this.headers[":method"]
    }
    get url() {
      return this.headers[":path"]
    }
    set url(value) {
      this.headers[":path"] = value
    }
    get authority() {
      return this.headers[":authority"]
    }
    get scheme() {
      return this.headers[":scheme"]
    }
    get httpVersion() {
      return "2.0"
    }
    get httpVersionMajor() {
      return 2
    }
    get httpVersionMinor() {
      return 0
    }
    get socket() {
      return this[kStream].session?.socket
    }
    get connection() {
      return this.socket
    }
    setTimeout(ms, cb) {
      this[kStream].setTimeout(ms, cb)
      return this
    }
  }
  class Http2ServerResponse extends Stream {
    constructor(stream, options) {
      super()
      this[kStream] = stream
      this[kHeaders] = Object.create(null)
      this[kTrailers] = null
      this.statusCode = 200
      this.statusMessage = ""
      this.sendDate = true
      this.finished = false
      this.writable = true
      stream.on("wantTrailers", () => stream.sendTrailers(this[kTrailers] ?? {}))
      stream.on("close", () => {
        this.writable = false
        this.emit("close")
      })
      stream.on("finish", () => this.emit("finish"))
      stream.on("drain", () => this.emit("drain"))
      stream.on("error", (e) => {
        if (this.listenerCount("error")) this.emit("error", e)
      })
    }
    get stream() {
      return this[kStream]
    }
    get socket() {
      return this[kStream].session?.socket
    }
    get connection() {
      return this.socket
    }
    get headersSent() {
      return this[kStream].headersSent
    }
    get writableEnded() {
      return this.finished
    }
    get writableFinished() {
      return this[kStream].writableFinished
    }
    setHeader(name, value) {
      if (this.headersSent) throw codedError("ERR_HTTP2_HEADERS_SENT", "Response has already been initiated.")
      this[kHeaders][String(name).toLowerCase()] = value
      return this
    }
    appendHeader(name, value) {
      const key = String(name).toLowerCase()
      const prev = this[kHeaders][key]
      this[kHeaders][key] = prev === undefined ? value : [].concat(prev, value)
      return this
    }
    getHeader(name) {
      return this[kHeaders][String(name).toLowerCase()]
    }
    getHeaders() {
      return { ...this[kHeaders] }
    }
    getHeaderNames() {
      return Object.keys(this[kHeaders])
    }
    hasHeader(name) {
      return String(name).toLowerCase() in this[kHeaders]
    }
    removeHeader(name) {
      delete this[kHeaders][String(name).toLowerCase()]
    }
    addTrailers(headers) {
      this[kTrailers] = { ...(this[kTrailers] ?? {}), ...headers }
    }
    writeHead(statusCode, statusMessage, headers) {
      if (typeof statusMessage === "object" && statusMessage !== null) headers = statusMessage
      this.statusCode = statusCode
      if (Array.isArray(headers)) {
        for (let i = 0; i < headers.length; i += 2) this.setHeader(headers[i], headers[i + 1])
      } else if (headers) for (const [k, v] of Object.entries(headers)) this.setHeader(k, v)
      this.#respond(false)
      return this
    }
    #respond(endStream) {
      const stream = this[kStream]
      if (stream.headersSent || stream.destroyed) return
      const headers = { ...this[kHeaders], ":status": this.statusCode }
      stream.respond(headers, { endStream: false, waitForTrailers: true, sendDate: this.sendDate })
    }
    write(chunk, encoding, cb) {
      if (typeof encoding === "function") [encoding, cb] = [undefined, encoding]
      this.#respond(false)
      return this[kStream].write(chunk, encoding, cb)
    }
    end(chunk, encoding, cb) {
      if (typeof chunk === "function") [chunk, encoding, cb] = [undefined, undefined, chunk]
      else if (typeof encoding === "function") [encoding, cb] = [undefined, encoding]
      if (this.finished) {
        if (typeof cb === "function") queueMicrotask(cb)
        return this
      }
      this.finished = true
      this.#respond(false)
      if (typeof cb === "function") this.once("finish", cb)
      if (chunk !== undefined && chunk !== null) this[kStream].end(chunk, encoding)
      else this[kStream].end()
      return this
    }
    flushHeaders() {
      this.#respond(false)
    }
    writeContinue() {
      this[kStream].additionalHeaders({ ":status": 100 })
    }
    writeEarlyHints(hints) {
      this[kStream].additionalHeaders({ ":status": 103, ...hints })
    }
    createPushResponse(headers, cb) {
      queueMicrotask(() => cb(codedError("ERR_HTTP2_PUSH_DISABLED", "HTTP/2 client has disabled push streams")))
    }
    setTimeout(ms, cb) {
      this[kStream].setTimeout(ms, cb)
      return this
    }
    destroy(err) {
      this[kStream].destroy(err)
    }
  }

  const setupServer = (server, options, onRequest, secure) => {
    server[kOptions] = options
    server.timeout = 0
    if (typeof onRequest === "function") server.on("request", onRequest)
    server.on(secure ? "secureConnection" : "connection", (socket) => {
      if (secure && socket.alpnProtocol && socket.alpnProtocol !== "h2" && !options.allowHTTP1) {
        socket.destroy()
        return
      }
      const session = new ServerHttp2Session(options, socket, server)
      if (server.timeout) session.setTimeout(server.timeout)
      session.on("stream", (stream, headers, flags, raw) => {
        server.emit("stream", stream, headers, flags, raw)
        if (server.listenerCount("request")) {
          const Request = options.Http2ServerRequest ?? Http2ServerRequest
          const Response = options.Http2ServerResponse ?? Http2ServerResponse
          const req = new Request(stream, headers, options, raw)
          const res = new Response(stream, options)
          if (headers.expect === "100-continue" && server.listenerCount("checkContinue")) server.emit("checkContinue", req, res)
          else server.emit("request", req, res)
        }
      })
      session.on("error", (e) => {
        if (server.listenerCount("sessionError")) server.emit("sessionError", e, session)
      })
      session.on("timeout", () => server.emit("timeout", session))
      server.emit("session", session)
    })
  }
  class Http2Server extends net.Server {
    constructor(options, onRequest) {
      if (typeof options === "function") [options, onRequest] = [{}, options]
      super()
      setupServer(this, options ?? {}, onRequest, false)
    }
    setTimeout(ms, cb) {
      this.timeout = ms
      if (typeof cb === "function") this.on("timeout", cb)
      return this
    }
    updateSettings(settings) {
      Object.assign((this[kOptions].settings ??= {}), settings)
    }
  }
  class Http2SecureServer extends tls.Server {
    constructor(options, onRequest) {
      if (typeof options === "function") [options, onRequest] = [{}, options]
      options = options ?? {}
      super({ ...options, ALPNProtocols: options.ALPNProtocols ?? (options.allowHTTP1 ? ["h2", "http/1.1"] : ["h2"]) })
      setupServer(this, options, onRequest, true)
    }
    setTimeout(ms, cb) {
      this.timeout = ms
      if (typeof cb === "function") this.on("timeout", cb)
      return this
    }
    updateSettings(settings) {
      Object.assign((this[kOptions].settings ??= {}), settings)
    }
  }

  return {
    connect,
    constants,
    createServer: (options, onRequest) => new Http2Server(options, onRequest),
    createSecureServer: (options, onRequest) => new Http2SecureServer(options, onRequest),
    getDefaultSettings: defaultSettings,
    getPackedSettings: (settings) => packSettings(settings ?? defaultSettings()),
    getUnpackedSettings: (buf) => unpackSettings(Buffer.from(buf)),
    performServerHandshake: (socket, options) => new ServerHttp2Session(options ?? {}, socket, null),
    sensitiveHeaders,
    Http2ServerRequest,
    Http2ServerResponse,
    Http2Session,
    ServerHttp2Session,
    ClientHttp2Session,
    Http2Stream,
    ServerHttp2Stream,
    ClientHttp2Stream,
  }
})
