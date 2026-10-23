# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- TypeScript runs without a build step: `.ts`, `.mts` and `.cts` files are parsed with their
  types stripped, including generics, `as`/`satisfies`/`!`, overloads, abstract classes,
  access modifiers, `declare`, index signatures and type-only imports, which are removed together
  with imports that are only used as types. Constructor parameter properties, `enum`, `const
  enum` and `namespace` are compiled like `tsc` does, and CommonJS files accept `import x =
  require()` and `export =`. `effect`, `rxjs`, `zod` and `chrono-node` run from their TypeScript
  sources.
- JSX in `.jsx` and `.tsx` files, compiled to the automatic `react/jsx-runtime` by default or to
  classic factory calls, configured by `jsx`, `jsxFactory`, `jsxFragmentFactory` and
  `jsxImportSource` in `tsconfig.json`/`jsconfig.json` or by `@jsx` comment pragmas; React server
  rendering gives the same HTML as Node with esbuild, and the TSX of shadcn/ui and Excalidraw
  parses.
- TypeScript legacy decorators: with `experimentalDecorators` in `tsconfig.json`, class, method,
  accessor, property and parameter decorators compile like `tsc` emits them, and
  `emitDecoratorMetadata` records `design:type`, `design:paramtypes` and `design:returntype` for
  `reflect-metadata`. A NestJS application with dependency injection, controllers and route
  parameters runs from its TypeScript sources.
- `./x.js` imports find `x.ts` when only the TypeScript file exists, extensionless imports try the
  TypeScript extensions, and `tsconfig.json`/`jsconfig.json` `paths` and `baseUrl` (with
  `extends`, comments and trailing commas) resolve path aliases such as `@/lib/x`.

- `vrotdaem` installs `optionalDependencies`, skipping packages whose `os`, `cpu` or `libc` do not
  match the machine, and keeps the executable bit of files on Linux and macOS, so the native
  `esbuild` package runs its platform binary.
- `vrotdaem` links the `bin` commands of installed packages into `nude_modules/.bin` and runs
  `preinstall`, `install` and `postinstall` scripts of packages listed in `trustedDependencies`
  in `squirt.json`, with `node` resolving to `jsq`. `vrotdaem trust <name>` adds a package to the
  list and runs its scripts, and `gluboko` names the packages whose scripts it skipped.
- `TextDecoder` decodes every encoding of the WHATWG Encoding Standard, including
  `windows-1251`, `koi8-r`, the ISO 8859 family, `gbk`, `gb18030`, `big5`, `shift_jis`, `euc-jp`
  and `euc-kr`, with Node's encoding names and `ERR_ENCODING_INVALID_ENCODED_DATA` in fatal mode.
- `fs.opendir`, `fs.opendirSync` and `fs.promises.opendir` return a `Dir` with `read`, `readSync`,
  `close`, async iteration and disposal, so `fs-extra` copies directories.
- `process.getBuiltinModule(id)` returns a core module or `undefined`, which `h3` relies on.
- `crypto.subtle` wraps and unwraps keys with AES-KW (RFC 3394) and generates AES-KW keys, so
  `openpgp` encrypts and decrypts messages.
- Native addons: `require`, `import` and `process.dlopen` load `.node` files built on Node-API
  version 9, with wrapped objects and finalizers, references, promises, async work, threadsafe
  functions called from native threads, buffers and `BigInt`, plus the libuv thread primitives
  that addons call directly. `@swc/core`, `lightningcss`, `@tailwindcss/oxide`, `rollup`, `sharp`,
  `@napi-rs/canvas`, `@parcel/watcher`, `classic-level` and `msgpackr` run. When a package ships a
  V8-specific build next to a Node-API one, the Node-API build is loaded, and addons that only
  exist for the V8 C++ API fail with `ERR_DLOPEN_FAILED` and an explanation.

### Fixed

- Packages in `node_modules` were not found; bare specifiers now look in `node_modules` as well as
  `nude_modules`, so projects installed with npm, pnpm or yarn run unchanged.
- `module.createRequire(filename)` returned the caller's `require`; it now resolves relative to the
  given path or `file:` URL and has its own `resolve`.
- `http.Server.listen` lost its callback when it was called with a backlog or an explicit
  `undefined` host, which is how `ws` starts `new WebSocketServer({ port })`; every Node form of
  the call works now.
- `fetch` removed `content-encoding` and `content-length` from compressed responses, and
  `http.request` decompressed bodies that Node hands over raw. `fetch` now asks for
  `gzip, deflate` like Node, decodes gzip, deflate (zlib and raw) and brotli while keeping the
  headers, and `node:http` clients receive the bytes the server sent.
- `ServerResponse` has Node's `_implicitHeader()`, which `express-session` and `compression` call.
- `TextDecoder` with `{ stream: true }` dropped characters split between chunks, turning them
  into replacement characters; streaming now keeps the partial sequence until the next call.
- A CommonJS module imported from an ES module exposes a `"module.exports"` export like Node 24,
  and `export *` passes it on, so `require()` of an ES module that re-exports a CommonJS file
  returns that file's `module.exports`. `require("mocha")` returns the `Mocha` class again.
- HTTP and TCP servers have Node's `_handle` while they listen, so `supertest` closes the server it
  starts and the process exits when the tests finish.
- On Windows a connection to a closed local port took two seconds to fail, because Windows
  retries refused connections; `net`, `http` and `fetch` now fail at once like Node, which also
  removes a two-second pause when socket.io clients shut down.
- `fetch` errors carry Node's `code`, `syscall` and `errno` on their `cause`, such as
  `ECONNREFUSED` and `ENOTFOUND`.
- Closing an HTTP server ran its callback on every later `close` event, and closing a server that
  was not running did not report `ERR_SERVER_NOT_RUNNING`.
- `node:http2` exports `Http2ServerRequest`, `Http2ServerResponse` and the session and stream
  classes, so `@hono/node-server` no longer answers every request with status 400.
- On glibc Linux `vrotdaem` also installed the musl builds of platform packages, because the
  registry metadata leaves out `libc`; the `package.json` of each optional package is now checked.

## [1.5.0] - 2026-10-23

### Added

- `worker_threads.Worker` runs each worker on its own operating-system thread with a separate
  heap, so CPU-bound work runs in parallel. Messages are structured clones that keep `Date`,
  `Map`, `Set`, typed arrays, errors, holes and cycles, transfer lists move `ArrayBuffer`s and
  `MessagePort`s between threads, `SharedArrayBuffer` memory is shared, and the `Atomics`
  operations are real atomic instructions with `Atomics.wait` and `Atomics.notify` blocking and
  waking across threads. `terminate()` interrupts running code, `process.exit` in a worker ends
  only that worker, uncaught errors reach the parent's `error` event with their class, and
  `receiveMessageOnPort`, `environmentData` and cross-thread `BroadcastChannel` work. `piscina`
  and `tinypool` run unchanged.
- `structuredClone` accepts `{ transfer }` and shares the memory of `SharedArrayBuffer`s.
- `node:crypto` handles asymmetric keys and ciphers: `KeyObject`s for RSA, RSA-PSS, P-256,
  P-384, Ed25519 and X25519 imported from and exported to PEM, DER and JWK (PKCS#8, SPKI,
  PKCS#1, SEC1, encrypted PKCS#8, certificates), `generateKeyPair`, `sign`/`verify` and
  `createSign`/`createVerify`, AES CBC/CTR/ECB/GCM and ChaCha20-Poly1305 ciphers, RSA
  `publicEncrypt`/`privateDecrypt`, ECDH, X25519, `diffieHellman` and `scrypt`. `crypto.subtle`
  gained ECDSA, Ed25519, RSASSA-PKCS1-v1_5, RSA-PSS, RSA-OAEP, AES-GCM/CBC/CTR, ECDH and X25519
  with `spki`, `pkcs8` and `jwk` formats and key wrapping. `jsonwebtoken` RS256/PS256/ES256 and
  `jose` JWS and JWE work.
- Regular expressions with the `v` flag accept nested classes, `&&` intersections and `\q{...}`
  string alternatives.
- `Atomics.waitAsync` returns a promise that settles when another thread notifies the location
  or the timeout passes; every `Atomics` test in test262 now passes, including the multi-agent
  ones.
- Explicit resource management: `using` and `await using` declarations in blocks, functions,
  loops, `for...of` heads and modules, `DisposableStack`, `AsyncDisposableStack`,
  `Symbol.dispose` and `Symbol.asyncDispose` methods on iterators and async iterators, and a
  `SuppressedError` that can be called without `new`. All test262 tests for the feature pass.

### Changed

- Strings are reference-counted through a thin 8-byte handle, which shrinks every JavaScript
  value from 24 to 16 bytes; programs holding large arrays and objects use about a quarter less
  memory. The cycle collector tracks objects with an intrusive index instead of weak references,
  so freed objects return their memory immediately.
- The lexer matches punctuators by their first character, interns identifiers and only checks
  for a regular expression when it sees a `/`, and the parser no longer rescans parenthesized
  code to look for `=>`; lexing is about twice as fast and `require("typescript")` takes about
  a third less time.

### Fixed

- `process.memoryUsage()` reported zero for every field; it now returns the resident set size
  and the allocator's committed memory.
- Events from unreferenced handles, such as an `unref`'d worker or socket, are delivered while
  the program waits for a timer instead of after it fires.
- `events.once` works with emitters that have `once` but no `removeListener`, as in Node.
- A `MessagePort` with a `message` listener keeps the program alive until it is closed or
  unreferenced, and removing the last listener releases it, matching Node.
- CommonJS modules are parsed as sloppy-mode scripts like in Node instead of as strict code, so
  legacy octal literals, `with`, implicit globals and identifiers named `await`, `yield` or
  `let` work, and `this` in plain function calls is the global object. Sloppy functions that
  never read `this` skip the global-object substitution, so calls cost the same as in strict
  code.
- A rejected top-level `await` in the entry module is reported as an uncaught exception and
  exits with status 1 even when an `unhandledRejection` listener is installed, as in Node.
- TCP sockets read and write on separate threads, so a write goes out immediately instead of
  waiting up to 20 ms for the reader to time out; a request-response round trip over `node:net`
  dropped from about 60 ms to under 0.1 ms and keep-alive `http.get` from 21 ms to about 1 ms.
- `fetch` and `http.get` deliver response bodies as they arrive instead of after the whole body
  is downloaded, and `node:http` servers stream responses that are written over time, so
  server-sent events and streamed API responses work in both directions. A client disconnect
  fires `close` on the request and response, and `request.signal` aborts in `serve()`.
- `server.listen()` without a host binds to `::` with IPv4 accepted as well, like Node, instead
  of `127.0.0.1` for `node:http` and `0.0.0.0` for `node:net`, so servers are reachable from
  other machines and containers and through `localhost` on IPv6. Connecting to a host with both
  IPv6 and IPv4 addresses tries them in parallel, which removes a two-second stall on Windows
  when a server listens only on `127.0.0.1`.
- Express's 404 handler crashed with `req.unpipe is not a function`; requests now have
  `unpipe`, `isPaused`, `read`, `unshift` and `setTimeout`.
- `ReadableStream` and `WritableStream` call `start()` synchronously from the constructor, as
  the streams standard requires.

## [1.4.0] - 2026-10-23

### Added

- `WebAssembly` on the `wasmi` interpreter: `validate`, `compile`, `instantiate`, the streaming
  variants, `Module` reflection (`imports`, `exports`, `customSections`), `Instance`, `Memory`
  with a zero-copy `buffer` that detaches on growth, `Table`, `Global`, JavaScript imports,
  `BigInt` for `i64`, and `CompileError`, `LinkError` and `RuntimeError` with V8's trap
  messages. SIMD is enabled, so `undici` runs its `llhttp` parser and its `fetch`, `request`,
  `Pool` and `Agent` work unchanged.
- `node:assert` was rebuilt after Node 24: failure messages show `+ actual - expected` Myers
  diffs of the inspected values, `assert(expr)` quotes the failing expression from the source,
  `deepStrictEqual` compares symbol keys, boxed primitives, errors, typed arrays, `ArrayBuffer`s
  and object keys of `Map`s and `Set`s and handles cycles, `assert/strict` and `assert.strict`
  map `equal` to `strictEqual`, and `partialDeepStrictEqual`, `CallTracker` and `Assert` exist.
- `Intl.NumberFormat` follows ECMA-402: exact decimal formatting of strings and `BigInt`s,
  `roundingMode`, `roundingIncrement`, `roundingPriority`, `trailingZeroDisplay`, all digit
  options with their defaults and errors, eighty numbering systems through `numberingSystem` or
  `-u-nu-`, compact notation for German, Japanese, Korean, Chinese and Indian English,
  `formatRange` and `formatRangeToParts` with ICU's collapsing and approximately sign,
  locale-specific currency symbols, and the legacy `Intl.NumberFormat.call(obj)` fallback.
- `Intl.Collator` resolves `collation`, `numeric` and `caseFirst` from options and `-u-co`,
  `-u-kn` and `-u-kf`, compares canonically equivalent strings as equal, honours the `base`,
  `accent`, `case` and `variant` sensitivities, and sorts German phonebook order.
- `util.inspect` accepts `sorted` and `maxArrayLength`, and prints objects that inherit from
  `Error.prototype` as errors.
- `stream.isDisturbed`, `stream.isErrored`, `readable.readableDidRead`, `readableAborted` and
  `errored` on readable and writable streams.

- TLS servers: `https.createServer({ key, cert })`, `tls.createServer` with `secureConnection`
  events and `ALPNProtocols`, and `serve({ tls: { key, cert } })`, all from PEM keys and
  certificates on top of rustls. TLS clients accept `ca` and `rejectUnauthorized: false` in
  `tls.connect`, `https.request` (also through `agent.options`) and `fetch(url, { tls })`, and
  `NODE_TLS_REJECT_UNAUTHORIZED=0` disables verification as in Node.
- `Symbol.toStringTag` on `URL`, `URLSearchParams`, `Headers`, `Request`, `Response`,
  `AbortController`, `AbortSignal`, `Event`, `EventTarget`, `DOMException`, `TextEncoder`,
  `TextDecoder`, `crypto`, `crypto.subtle` and `performance`, so type checks based on
  `Object.prototype.toString` (such as `got`'s URL validation) recognise them.

- IANA time zones in `Intl.DateTimeFormat` and the `toLocale*String` methods: the `timeZone`
  option accepts any zone from the bundled tz database (case-insensitively), fixed offsets such
  as `+05:30` and the UTC aliases, invalid names throw a `RangeError`, `timeZoneName` renders
  abbreviations and GMT offsets, `resolvedOptions().timeZone` reports the system zone, and
  `Intl.supportedValuesOf("timeZone")` lists the canonical zones.

- `util.inspect` honours `depth` (including `null` and `Infinity`), `breakLength` and
  `compact`, prints `[AsyncFunction]`, `[GeneratorFunction]` and `[AsyncGeneratorFunction]`
  tags, `SharedArrayBuffer`, `WeakRef {}` and `<Buffer >` like Node, and calls
  `Symbol.for("nodejs.util.inspect.custom")` methods; `URL`, `URLSearchParams` and `Headers`
  implement them, so `console.log(new URL(...))` shows every component.
- `util.format` follows Node for `%s` (user-defined `toString` wins, objects are inspected one
  level deep), `%o` (depth 4) and BigInt arguments to `%d`, `%i` and `%f`.
- `Readable.prototype.map`, `filter`, `flatMap`, `take`, `drop`, `forEach`, `reduce`, `some`,
  `every` and `find`; `setEncoding` decodes multi-byte characters split across chunks with a
  `StringDecoder`.
- `Intl.DateTimeFormat` gained `era`, `dayPeriod`, `hourCycle`/`hour12` in `resolvedOptions`,
  Node's GMT offset names for zones without a well-known abbreviation, and corrected `en-GB`
  and `pt-BR` patterns; `Intl.RelativeTimeFormat` speaks Ukrainian, Russian, German, French,
  Spanish and Polish; `Intl.ListFormat` knows the conjunctions of fifteen locales;
  `Intl.PluralRules` covers Arabic, Hebrew, Czech and Slovak; `Intl.NumberFormat` supports
  `currencySign: "accounting"`, Indian digit grouping for `en-IN`, the full-width yen sign in
  Japanese and three fraction digits in engineering notation.
- `path.toNamespacedPath`, `util.types.isProxy`, lenient `querystring.unescape`, `blob:`
  origins in `URL`, and `ERR_OUT_OF_RANGE`/`ERR_INVALID_ARG_TYPE` codes on `Buffer` errors.
- `Readable.toWeb`/`fromWeb`, `Writable.toWeb`/`fromWeb` and `Duplex.toWeb`/`fromWeb` bridge
  Node streams and WHATWG streams; `fs` streams report `writableEnded`, `writableFinished`,
  `readableEnded`, `destroyed` and `closed`.
- `node:child_process` returns Buffers from `execSync`, `spawnSync` and the new `execFileSync`
  unless an `encoding` is given, `spawnSync` reports a missing program through `error.code`
  `ENOENT` instead of throwing, and `spawnSync` results carry `output`, `signal` and `pid`.
- `Intl.Collator` sorts `å`, `ä`, `ö` (Swedish, Finnish) and `æ`, `ø`, `å` (Danish, Norwegian)
  after `z`; `supportedLocalesOf` drops unknown languages and unknown locales resolve to
  `en-US`.
- `process.memoryUsage.rss()`, `process.stdout.writable`, and `isTTY` is only defined when the
  stream is a terminal, as in Node.
- `toLocaleUpperCase`/`toLocaleLowerCase` apply the Turkish and Azerbaijani dotted and dotless
  `i` rules, and `BigInt.prototype.toLocaleString` formats every digit instead of rounding
  through a `Number`.

### Changed

- `Promise.resolve` and `Promise.reject` on the built-in `Promise` skip the generic
  capability machinery, which makes `await Promise.resolve(x)` about five times faster.

### Fixed

- `WeakRef.prototype.deref()` returned `undefined` as soon as the creating expression finished
  because nothing kept the target alive; targets now survive until the current job and its
  microtasks complete, as the specification requires.
- `require` reads `module.exports` through accessors, so modules that define it with a getter
  (such as `undici`'s embedded WebAssembly) export the right value.
- A `.js` file of unknown type that declares `module`, `exports`, `require`, `__filename` or
  `__dirname` at the top level is loaded as an ES module, as Node's syntax detection does,
  instead of failing with a redeclaration error.
- Errors list `stack` before `message` among their own properties, as in V8.
- `http.request` no longer follows redirects itself: like Node it hands `3xx` responses to the
  caller, and redirect-following libraries such as `axios` and `got` report the right final URL.
  Request failures carry Node's error codes (`ECONNREFUSED`, `ENOTFOUND`,
  `DEPTH_ZERO_SELF_SIGNED_CERT`, `UNABLE_TO_VERIFY_LEAF_SIGNATURE`, `ECONNRESET`).
- Running a CommonJS `.js` file directly (`jsq app.js`) executed it twice, once as an ES module
  and once as CommonJS while collecting its exports. The entry point now runs once through the
  CommonJS loader, so `__dirname`, `__filename`, `module`, `exports` and
  `require.main === module` work as in Node; entry files with `import`, `export` or top-level
  `await` still run as ES modules.
- `Number.prototype.toString(radix)` uses V8's digit generation, so `(0.5).toString(3)` and
  `(1e21).toString(36)` match Node exactly instead of printing extra digits.
- `JSON.parse` reports V8's messages: `Unexpected end of JSON input`, `Unexpected token ']',
  "[1,]" is not valid JSON`, `Expected property name or '}' in JSON at position 1 (line 1
  column 2)` and the rest.
- `structuredClone` rejects uncloneable values with a real `DOMException` named
  `DataCloneError`; the month in `weekday: "short", month: "short"` formats is abbreviated.
- `process.nextTick` callbacks run after the microtask queue drains, as in Node ES modules, so
  `await` continuations are not interleaved with ticks.
- `toFixed` and `toPrecision` round exact ties up and everything else by the exact binary value,
  so `(1.45).toFixed(1)` is `1.4` and `(2.5).toPrecision(1)` is `3`, matching V8.
- Error messages match V8 for destructuring `null` (`Cannot destructure property 'a' of
  'null'`), invalid `URL`s (`Invalid URL` with `code` and `input`), `atob`, radix and weak map
  keys, strict-mode reserved words, and Map/Set methods called on the wrong receiver; assigning
  an error's `name` or `message` updates the first line of its `stack`, and `util.inspect`
  prints a custom `stack` string as is.
- `Array.prototype.flat` skips holes, `Writable.prototype.write` reports back-pressure from the
  buffered byte count, and `stream.pipeline` settles when a `PassThrough` destination finishes
  even if nobody reads it.
- `Date.parse` accepts the slash-separated legacy formats V8 understands, such as
  `7/1/2026, 9:00:00 PM`, `2026/07/01 21:00:00` and `12/31/99`, so `new Date(date.toLocaleString("en-US", { timeZone }))`
  round-trips and the `dayjs` and `luxon` time zone plugins work.

## [1.3.0] - 2026-10-22

### Fixed

- A `var` inside a named function expression that reuses the function's own name shadows it
  instead of failing with `Assignment to constant variable` (`bn.js`, `libphonenumber-js`).
- `await` on a `Promise` subclass goes through its overridden `then`, as the specification's
  `PromiseResolve` requires, so SDKs built on custom promise classes (`openai`) resolve to the
  parsed response instead of `null`.
- `export { default as x } from "node:…"` and `export * as ns from "node:…"` re-export the
  builtin's bindings (`vfile`, `unified`).
- `process.env` is case-insensitive on Windows, as in Node, so `process.env.PATH` works when
  the variable is spelled `Path`; `String(process)` is `[object process]`.
- `child_process` honours a string `shell` option such as `powershell.exe` (`zx`).
- `http.IncomingMessage` carries `trailers`/`rawTrailers` and `ServerResponse` exposes the
  serialized `_header` once headers are sent (`@hapi/shot`, `minipass-fetch`).
- The `zlib` stream classes are function constructors whose `_handle.writeSync` drives
  synchronous inflation (`pngjs`), and `Buffer.alloc`, `allocUnsafe` and `allocUnsafeSlow` can
  be called with `new` (`cbor-x`, `msgpackr`).
- `crypto.generateKeyPair`/`generateKeyPairSync` are no longer defined, so libraries that
  feature-detect them fall back to their own implementations (`node-forge`).
- Named imports from a CommonJS module whose `module.exports` is a function (such as
  `readable-stream`) now see the function's own properties, matching Node's lexer-based named
  exports.
- `\uD800`-style escapes and `\u{...}` surrogate code points in regular expressions match the
  lone surrogates stored in strings instead of failing to compile.
- `vm.runInContext` and `Script.prototype.runInContext` bind `this` to the context and resolve
  `globalThis` through it, so a window-like context that assigns `this.Map = globalThis.Map` no
  longer overwrites the real globals (`happy-dom` destroyed `Map`, `Promise` and `Object`).
- `zlib.gunzip` and the tarball extractor in `vrotdaem` ignore bytes after the last gzip member,
  so block-padded tarballs such as `sylvester-0.0.21.tgz` install instead of failing with
  `invalid gzip header`; a failed `vrotdaem add` no longer leaves the dependency in `squirt.json`.
- `http.ServerResponse` and `http.IncomingMessage` are function constructors with real
  `once`/`off`/`listenerCount` semantics, `assignSocket`, `appendHeader` and `getHeaderNames`, so
  `util.inherits` subclasses that call them (`light-my-request`, and therefore `fastify.inject`)
  work.

### Added

- `crypto.subtle` gained `importKey` (`raw` and `jwk` symmetric keys), `exportKey`,
  `generateKey`, `sign`/`verify` with HMAC, `deriveBits`/`deriveKey` with PBKDF2 and HKDF, and
  a global `CryptoKey`; encryption algorithms report `NotSupportedError`. `jose` signs and
  verifies HS256 tokens.
- `setTimeout`, `setInterval` and `setImmediate` return Node-style `Timeout` and `Immediate`
  objects with `ref`, `unref`, `hasRef`, `refresh`, `close` and `Symbol.toPrimitive`, and
  unreferenced timers no longer keep the process alive.
- `stream.getDefaultHighWaterMark` and `setDefaultHighWaterMark`; `events.addAbortListener`
  and `events.EventEmitterAsyncResource` (`execa` runs child processes, `piscina` loads);
  `http.maxHeaderSize`; `performance.eventLoopUtilization`, `nodeTiming` and `timerify`, and
  `perf_hooks.createHistogram`/`monitorEventLoopDelay` (`@hapi/hapi`).
- The `zlib` stream classes expose the `_handle` and `_processChunk` internals that `minizlib`
  drives synchronously, so `tar` creates and extracts `.tgz` archives.
- `fs.watch` (polling a file or directory tree on an IO thread, with `recursive`, `persistent`,
  `signal`, `encoding`, `ref`/`unref` and `close`), `fs.watchFile`/`unwatchFile` with
  `StatWatcher`, and `fs.promises.watch` as an async iterator.
- File descriptor APIs in `fs`: `openSync`/`open`, `closeSync`/`close`, `readSync`/`read`,
  `writeSync`/`write`, `fstatSync`/`fstat`, `ftruncateSync`, `fsyncSync`, the `O_*` and `S_IF*`
  constants, and `fs.promises.open` returning a `FileHandle` with `read`, `write`, `readFile`,
  `writeFile`, `stat`, `truncate`, `createReadStream`, `createWriteStream`, `readLines` and
  `close`; packages such as `tar` that read and write through descriptors work.
- `http.createServer` servers emit `upgrade` with the request, the raw `net.Socket` and any
  bytes already read when a client asks for an HTTP upgrade, so WebSocket servers such as `ws`
  and `socket.io` run on top of them; connections without an `upgrade` listener are closed.

### Changed

- `RegExp.prototype.test` matches without collecting capture groups, which makes validation
  loops over large inputs noticeably faster.

## [1.2.0] - 2026-10-22

### Added

- `vrotdaem add <name>` installs packages from the npm registry: `vrotdaem add lodash`,
  `lodash@^4.17`, `@scope/pkg@1.x` or `npm:<name>@<range>`, with semver ranges (`^`, `~`, `x`,
  `*`, hyphens, `||`, comparators, dist-tags), tarball download and extraction without external
  tools, recursive installation of the packages' own dependencies and nested copies when
  versions conflict. The registry can be changed with `JSQ_NPM_REGISTRY`.
- `node:dns` with `lookup`, `resolve`, `resolve4` and `resolve6`; `node:net` and `node:tls`
  with `isIP` and friends plus socket and server classes that report `ENOTSUP` instead of
  failing to load, so packages that merely import them work; `util.formatWithOptions`,
  `util.parseArgs`, `util.stripVTControlCharacters`, `util.debuglog`, `util.styleText`,
  `util.aborted`, `util.toUSVString`, `util.getSystemErrorName` and `util.inspect.custom`.
- `node:vm`, `node:async_hooks` (synchronous `AsyncLocalStorage`), `node:punycode`,
  `node:diagnostics_channel`, `node:constants`, and loadable `node:http2`,
  `node:worker_threads`, `node:cluster`, `node:v8` and `node:inspector` whose unsupported
  operations throw `ENOTSUP`.
- `Error.prepareStackTrace` receives an array of call sites with `getFileName`,
  `getLineNumber`, `getFunctionName`, `isToplevel`, `isEval`, `isConstructor` and the other V8
  methods, `Error.stackTraceLimit` bounds every stack, and `Error.captureStackTrace(obj, fn)`
  hides the frames above `fn`.
- Module paths on Windows no longer carry the `\\?\` prefix.
- `require` resolves `exports` with the `require` condition and `import` with the `import`
  condition, so dual packages load the right build; `http.createServer` servers have the full
  EventEmitter surface, `setTimeout` and connection helpers; `http` exports `IncomingMessage` and
  `ServerResponse`; the Node-style `EventEmitter` stores listeners in `_events`, can be called as
  a plain function from `util.inherits` subclasses and mixed into other objects, and offers
  `errorMonitor`, `getEventListeners`, `EventEmitter.on` with abort signals.
- `process.version`, `process.versions.node` and `process.release.name` report a Node 22
  compatible runtime so that packages checking the Node version work; the JavaSquirt version
  stays in `process.versions.jsq` and `process.release.javasquirt`.
- A regular expression literal may follow `export default`.
- Package entry resolution honours `exports` conditions (`import`, `require`, `node`,
  `module-sync`, `default`), subpath exports with `*` patterns, and the `module` field.
- `process` is an event emitter: `on`, `once`, `off`, `emit`, `listenerCount` and friends work,
  `exit` fires with the exit code when the program ends or `process.exit` is called,
  `beforeExit` fires when the event loop drains and may schedule more work, `uncaughtException`
  and `unhandledRejection` listeners receive errors that would otherwise end the program, and
  `process.emitWarning` emits `warning` and prints `(jsq:pid) Name: message` to stderr unless
  `NODE_NO_WARNINGS=1`.
- `MessageChannel`, `MessagePort`, `BroadcastChannel`, `MessageEvent`, `CloseEvent` and
  `ErrorEvent` globals; ports deliver structured clones asynchronously and accept both
  `onmessage` and `on("message")`.
- `node:console` with the `Console` class that writes to any pair of streams,
  `node:util/types` with the full set of `is*` checks, `node:stream/web`,
  `node:stream/consumers` (`text`, `json`, `buffer`, `arrayBuffer`, `blob`), `node:test`
  (`test`, `describe`, `it`, hooks and `mock.fn`/`mock.method` on top of `squirt:test`),
  `node:timers/promises` with `setImmediate`, `setInterval` and `scheduler`, plus `node:sys`,
  `node:domain`, `node:trace_events`, `node:repl`, `node:sea` and `node:wasi` so that packages
  importing them load.
- `node:buffer` exports `SlowBuffer`, `kMaxLength`, `kStringMaxLength`, `constants`,
  `INSPECT_MAX_BYTES`, `atob`, `btoa`, `isUtf8`, `isAscii`, `transcode`, `Blob` and `File`, and
  the `Buffer` statics are enumerable like Node's so that `safer-buffer` style wrappers copy them.

- `process.hrtime.bigint`, `process.features`, `process.config`, `process.execArgv`,
  `process.argv0`, `process.ppid`, `process.title`, `process.umask`, `process.kill` (own pid
  only), `process.cpuUsage`, `process.resourceUsage`, `process.abort`, `process.report`,
  `process.setUncaughtExceptionCaptureCallback` and `process.getuid`/`getgid`/`geteuid`/`getegid`
  on Unix.
- `os.release`, `os.version`, `os.machine`, `os.userInfo`, `os.uptime`, `os.totalmem`,
  `os.freemem`, `os.loadavg`, `os.networkInterfaces`, `os.endianness`, `os.devNull`,
  `os.constants`, `os.getPriority` and `os.setPriority`.
- `crypto.KeyObject`, `createSecretKey`, `generateKeySync`, `pbkdf2`/`pbkdf2Sync`,
  `hkdf`/`hkdfSync`; `createHmac` accepts secret `KeyObject`s, and the asymmetric key, cipher
  and scrypt functions exist but throw errors with `code`s so that packages such as
  `jsonwebtoken` fall back to HMAC.
- `fs.realpath.native` and `fs.realpathSync.native`.
- Real TCP sockets: `net.connect`/`net.createConnection` and `net.Socket` are duplex streams
  over a socket thread with `connect`, `data`, `end`, `close`, `error`, `timeout` and `ready`
  events, `setTimeout`, `ref`/`unref`, `address`, `bytesRead`/`bytesWritten`;
  `net.createServer` accepts connections with `connection`, `listening` and `close` events,
  `address`, `maxConnections` and `getConnections`; `tls.connect` opens TLS 1.2/1.3 client
  connections verified against the Mozilla root store, with `secureConnect`.
- `vm.constants` with `DONT_CONTEXTIFY` and `USE_MAIN_CONTEXT_DEFAULT_LOADER`;
  `vm.createContext(vm.constants.DONT_CONTEXTIFY)` returns a plain context object.
- `zlib` exposes the Brotli and Zstandard constants, classes and functions; they report
  `ERR_ZLIB_UNSUPPORTED` when used, which lets `undici` and `jsdom` load.
- A global `WebSocket` client (RFC 6455 over `net`/`tls` and the HTTP upgrade path) with
  `open`, `message`, `close` and `error` events, `send` of strings, `ArrayBuffer`s, views and
  `Blob`s, `close(code, reason)`, `binaryType` (`blob`, `arraybuffer` or `nodebuffer`),
  `protocol`, `extensions`, `bufferedAmount` and the `readyState` constants.
- `http.request` and `https.request` with an `Upgrade` header (or `createConnection`) go over a
  raw `net`/`tls` socket and emit `upgrade` with the response head and the socket, so the `ws`
  WebSocket client works; `https.request(options)` defaults the protocol to `https:`.
- Asynchronous `child_process`: `spawn` with piped or inherited stdio, `stdin`/`stdout`/`stderr`
  streams, `spawn`, `exit` and `close` events and `kill`; `exec`, `execFile` (with `maxBuffer`,
  `timeout`, `encoding` and `util.promisify` support) and `fork` with an IPC channel
  (`child.send`, `child.on("message")`, `process.send`, `process.on("message")`, `disconnect`)
  carried over a loopback socket with JSON serialisation.
- `worker_threads.Worker` runs the worker file in a separate `jsq` process: `postMessage`,
  `on("message")`, `online`, `error` and `exit` events, `terminate`, `workerData`,
  `parentPort`, `isMainThread` and `threadId`; `SharedArrayBuffer` and transfer lists are not
  shared between the processes.
- `TextDecoder` accepts `utf-16le`, `utf-16be`, `latin1`, `ascii`, `iso-8859-1` and
  `windows-1252` labels.
- Import attributes: `import data from "./x.json" with { type: "json" }` and the legacy `assert`
  form parse on static imports and re-exports.
- `node:url` gained the legacy API: `parse`, `format`, `resolve`, `resolveObject`, `Url`,
  `domainToASCII`, `domainToUnicode` and `urlToHttpOptions`.
- `Buffer` gained `copy`, `lastIndexOf`, `swap16`/`swap32`/`swap64`, the `BigInt64` and
  variable-width `readIntLE`/`readUIntBE`-style readers and writers, `toLocaleString`, the
  `*Slice`/`*Write` helpers and `parent`/`offset`.
- `Readable`, `Writable`, `Duplex`, `Transform` and `PassThrough` are function constructors that
  initialise an existing object when called as `Readable.call(this, options)`, so
  `util.inherits`-style stream subclasses from `readable-stream`, `light-my-request` and older
  packages work.
- `require("stream")` is the legacy callable `Stream` constructor with `pipe`, and `Readable`
  and `Writable` extend it, so `readable-stream` and packages that call `Stream.call(this)` work.
- `\p{RGI_Emoji}` and the other emoji sequence properties under the `v` flag match emoji
  sequences (flags, keycaps, tag sequences, modifiers and ZWJ sequences).

### Fixed

- Assigning `fn.prototype` on an ordinary function whose prototype chain contains a class no
  longer throws, which fixes `markdown-it`'s callable wrapper.
- Unresolved identifiers fall back to `Object.prototype` members the way `globalThis` inherits
  them in Node, so `toString` or `hasOwnProperty` used as a bare name resolve instead of throwing.
- `\p{Surrogate}` in Unicode regular expressions matches nothing instead of failing to compile,
  so `string-width` and `yargs` load.
- `{ arguments }` as a shorthand property inside a function refers to the arguments object
  instead of throwing a ReferenceError.
- Compiling a scope with many declarations is linear instead of quadratic: local variables are
  indexed by name and the hoisting pass uses hash sets, so a 40,000-declaration bundle compiles in
  a fraction of a second instead of half a minute and `typescript`'s 8 MB library loads.
- `charCodeAt`, `charAt`, `codePointAt`, `at`, `slice`, `substring`, `substr`, `indexOf`,
  `includes`, `startsWith`, `endsWith`, indexing and `.length` on large strings use a cached
  UTF-16 index instead of rescanning the string, so scanners such as the TypeScript parser run in
  linear time (parsing `lib.es5.d.ts` went from two minutes to about a second).
- Capturing a variable from an enclosing function is logarithmic in the number of open
  upvalues instead of linear, which matters for bundles that define thousands of closures in one
  scope.
- `Object.freeze` and `Object.seal` leave private fields writable, as the specification requires,
  so frozen class instances such as ESLint's `SourceCode` can still update their private state.
- A `.js` file without `import`/`export` syntax is CommonJS unless the nearest `package.json`
  says `"type": "module"`, matching Node, so packages such as `xlsx` that export through
  `module.exports` detection load.
- Character class ranges whose bounds are out of order after surrogate mangling in non-Unicode
  regular expressions are emitted as single characters instead of failing to compile.
- `vrotdaem add name@range` replaces an installed top-level copy that does not satisfy the new
  range instead of reporting it as already installed.
- `require()` of an ES module returns a plain object with the exports, `__esModule: true` when
  there is a default export, or the value of a `"module.exports"` export, as Node does, so
  Babel-style interop (`mod.__esModule ? mod.default : mod`) picks the default export.
- A `.js` file without `import`/`export` that fails to compile as CommonJS because of top-level
  `await` or `import.meta` is loaded as an ES module, like Node's module detection.
- Running a file whose path needs Windows' `\\?\` form no longer evaluates it twice when it
  imports itself.
- Module syntax detection tokenises the file instead of scanning lines, so `import`/`export`
  inside strings, comments or template literals and dynamic `import()` no longer make a CommonJS
  bundle (such as `xlsx`, `typescript` or the `svelte` compiler) load as an ES module.
- `export * as name from "pkg"`, `export { x } from "pkg"` and `export * from "pkg"` now link
  when the source is a package or a CommonJS file; previously only relative ES modules were
  re-exported, which broke `htmlparser2`, `cheerio` and similar packages.
- `http` request objects report `readable` and `complete` correctly and expose a `socket` with
  the usual methods, so `body-parser` and `express.json()` read request bodies.
- `require()` of a built-in module no longer exposes an enumerable `default` key.

## [1.1.0] - 2026-10-22

### Added

- Install without Rust: `install.sh` for Linux and macOS and `install.ps1` for Windows download
  the latest release, verify its checksum and put `jsq` and `vrotdaem` on the `PATH`; a Scoop
  manifest in `bucket/` and a Homebrew formula in `Formula/` let the repository serve as a bucket
  and a tap; `scripts/update-manifests.sh` refreshes both from a release's checksums and the
  release workflow opens a pull request with that update.
- `node:crypto`: `createHash`, `createHmac`, `hash`, `getHashes`, `randomBytes`,
  `randomFillSync`, `randomInt`, `randomUUID`, `getRandomValues`, `timingSafeEqual` and
  `webcrypto`, with md5, sha1, sha224, sha256, sha384 and sha512; `crypto.subtle.digest` on the
  global `crypto`.
- `node:http` requests gained `setEncoding`, `rawHeaders`, `off`, `removeListener`, `emit`,
  `pause`, `resume`, `pipe`, `text()`, `json()` and `arrayBuffer()`, deliver body chunks as
  `Buffer` until an encoding is set, and responses gained `on("finish")`, `on("close")`,
  `flushHeaders`, `setTimeout` and `finished`.

- `fs.createReadStream` and `fs.createWriteStream`: event-based file streams with `setEncoding`,
  `pause`, `resume`, `pipe`, `destroy`, `start`/`end`/`flags` options and async iteration, so
  `readline.createInterface({ input: fs.createReadStream(file) })` reads a file line by line.
- `console.table`, `console.group`, `console.groupCollapsed`, `console.groupEnd`,
  `console.time`, `console.timeLog`, `console.timeEnd`, `console.count`, `console.countReset`,
  `console.dir`, `console.dirxml` and `console.trace`.

- `jsq -p "<code>"` evaluates a snippet and prints its value.
- Web streams: `ReadableStream`, `WritableStream` and `TransformStream` with default readers and
  writers, `pipeTo`, `pipeThrough`, `tee`, `cancel`, `abort`, abort signals, backpressure and
  async iteration, `ReadableStream.from`, `TextEncoderStream`, `TextDecoderStream`,
  `CompressionStream`, `DecompressionStream`, `CountQueuingStrategy` and
  `ByteLengthQueuingStrategy`; `Response.body` and `Request.body` are readable streams, stream
  bodies can be sent with `fetch` and returned from server handlers, and `Blob.prototype.stream`.
- `Blob`, `File` and `FormData`: construct from strings, buffers and other blobs, `slice`,
  `text`, `arrayBuffer` and `bytes`; `fetch`, `Request` and `Response` accept them as bodies,
  encode `FormData` as `multipart/form-data`, and expose `blob()` and `formData()` that also
  parse multipart and URL-encoded bodies.
- CommonJS: a global `require` loads `node:` and `squirt:` builtins, JSON files and `.js` or
  `.cjs` files wrapped with `exports`, `module`, `__filename` and `__dirname`, with
  `require.resolve`, `require.cache` and `node:module`'s `createRequire`; `import` of a `.cjs`
  file, or of a `.js` file that uses `module.exports` or `require` without `import` or `export`,
  gets its exports as the default export and as named exports; module resolution tries
  `.jsq`, `.js`, `.mjs`, `.cjs` and `.json` extensions, `index` files, and `package.json`
  `main` or `exports` next to `squirt.json`.
- Globals `setImmediate`, `clearImmediate`, `global`, `navigator`, `reportError` and
  `CustomEvent`; `performance.mark`, `measure`, `timeOrigin` and entry queries;
  `process.availableParallelism`.
- Node modules `string_decoder`, `querystring`, `tty` and `perf_hooks`.
- `node:stream`: `Readable`, `Writable`, `Duplex`, `Transform`, `PassThrough`, `Readable.from`,
  `pipeline`, `finished` and the `stream/promises` versions, with backpressure, `drain`,
  object mode, encodings and async iteration.
- `Intl.DateTimeFormat`, `toLocaleDateString`, `toLocaleTimeString` and `toLocaleString`
  format dates for Ukrainian, Russian, German, French, Spanish, Italian, Portuguese, Polish,
  Dutch, Swedish, Turkish, Japanese, Chinese and British, Australian and Canadian English, with
  localized month and weekday names, genitive forms, date orders, separators, 24-hour clocks and
  `dateStyle`/`timeStyle` patterns.
- `node:zlib`: gzip, zlib and raw deflate compression and decompression in sync, callback and
  stream forms, `unzip`, `crc32` and the `constants` table.
- `http.request`, `http.get`, `http.Agent` and the `node:https` module: client requests built
  on `fetch` whose responses are readable streams with `statusCode`, `headers`, `setEncoding`
  and `json()`.

### Fixed

- `Intl.DateTimeFormat`, `Intl.NumberFormat` and `Intl.Collator` can be called without `new`,
  and subclassing them keeps the subclass prototype.

### Changed

- `jsq -e` no longer prints the value of the last expression, matching `node -e`; use `-p`.
- `child_process` with a shell on Windows runs `cmd /d /s /c` with the command line passed
  verbatim, so quoted program paths and arguments work.
- `expect(...).toEqual` follows Jest: it ignores prototypes and `undefined` properties, so
  `Object.groupBy` results compare equal to plain object literals; `toStrictEqual` and
  `util.isDeepStrictEqual` keep checking prototypes.

## [1.0.0] - 2026-10-22

### Added

- Mapped `arguments`: in sloppy functions with a simple parameter list, `arguments[i]` and the
  i-th parameter are the same variable in both directions, also from closures and across
  generator suspensions. `delete arguments[i]`, defining an accessor and making an element
  non-writable break the link for that index, exactly as the specification describes, and
  `Object.freeze(arguments)` breaks it for every index.
- Global lexical environment: top-level `let`, `const` and `class` declarations in scripts no
  longer become properties of the global object. They shadow same-named global properties,
  stay visible to later scripts, indirect `eval` and `$262.evalScript`, and cannot be deleted.
  A script that redeclares a global lexical binding, declares `let` over a non-configurable
  global property or declares `var` or a function over a lexical binding fails with a
  `SyntaxError` before any of its declarations take effect; `var` and function declarations on a
  non-extensible global object fail with a `TypeError`.
- Function names are visible inside parameter default expressions, a `var` in the body that
  reuses a parameter name gets its own binding when the parameters have default values, a
  function declaration may reuse a parameter name, and a function declared in the body named
  `arguments` suppresses the arguments object.
- Annex B: `for (var x = init in obj)` runs the initializer once before the loop, `f() = v`,
  `f() += v`, `f()++` and `for (f() in obj)` evaluate the call and throw a `ReferenceError` at
  run time in sloppy code instead of failing to parse, `-->` on the first line of a script is a
  comment even after whitespace or delimited comments, and a block-level function named
  `arguments` is skipped by the hoisting rules.
- `for (let in obj)` treats `let` as an identifier in sloppy code, `await` is a valid label in
  scripts, escaped identifiers can be labels, ``new tag`template` `` constructs the result of
  the tagged template call and `new import.meta()` is parsed.
- `Function.prototype.caller` read on a sloppy function that is currently executing reports the
  sloppy function that called it, and `null` when the caller is strict, a generator, async,
  native or the top level.

### Fixed

- Assigning to an unresolvable name in strict code evaluates the right-hand side first and then
  throws a `ReferenceError`, even when the right-hand side created the binding.
- `typeof` on a global accessor property calls the getter.
- `symbol == object` converts the object with `Symbol.toPrimitive` before comparing.
- `for...in` skips properties deleted during the enumeration.
- Closures created in the first `for (let ...)` head expressions and in the update expression
  observe the per-iteration copy of the loop variable.
- Assigning a property on a primitive invokes setters and proxy traps found on its prototype
  chain, and throws a `TypeError` in strict code when nothing accepts the assignment.
- `let` and `const` cannot bind the name `let`, `let` followed by a newline and an identifier is
  a declaration, `catch (e) { function e() {} }` is an early error, and `"use strict"` written
  with escape sequences or line continuations is not a directive.
- Lexer: a lone carriage return terminates statements and string literals, line and paragraph
  separators end hashbang and block comments, and raw template strings normalise `\r\n` after
  a backslash to `\n`.
- Generator objects take their prototype after the parameters have been evaluated, so a default
  value that replaces `prototype` is honoured.
- Function declarations in a global `eval` update an existing configurable property to be
  writable and enumerable.
- `var x = e` inside `with` resolves `x` through the object before evaluating `e`, and `with`
  statements reset the completion value seen by `eval`.
- `Object.getOwnPropertyNames` on functions and classes lists integer keys first.
- `Date.prototype.setYear` truncates the year before deciding whether it is two-digit.
- `Object.prototype.toString` and `console.log` recognise mapped arguments objects.

## [0.10.0] - 2026-10-22

### Added

- `with` statement: identifiers, calls, `typeof` and `delete` inside the body and in closures
  created there resolve through the object first, honouring `Symbol.unscopables`, and strict code
  throws a `ReferenceError` when a binding disappears between lookup and use.
- Sloppy mode semantics: assigning to an undeclared name creates a global, failed assignments and
  deletes are ignored instead of throwing, `this` in a plain call is `globalThis`, `arguments`
  has `callee`, `caller` and `arguments` on plain functions read as `null`, legacy octal literals
  and escapes parse, `yield`, `let` and `await` work as identifiers, duplicate parameters and
  block-level duplicate functions are accepted, `if (x) function f() {}` and labelled function
  declarations work, and block-level functions hoist to the enclosing function following the
  Annex B rules for conflicting `let`, `const`, `class`, parameter and catch bindings.
- Direct `eval` declarations: `var` and function declarations inside a sloppy direct `eval` join
  the calling function's scope or the global object, including from nested evals, parameter
  default expressions and arrow functions; declaring a name that collides with a `let` or `const`
  of the caller, or `arguments` from a parameter expression, is a `SyntaxError`.
- Global declarations follow the specification: `var` and function declarations create
  non-configurable properties, `eval` creates configurable ones, redeclaring a non-configurable
  global function throws before anything is declared, `let undefined` is rejected and `delete`
  of a global reports its configurability.
- Early errors: legacy octal escapes and literals in strict code, numeric separators in legacy
  literals, `??` mixed with `||` or `&&`, rest elements that are not last, private names after
  `super` or in object patterns, `await` and `arguments` inside static blocks, `yield` and `await`
  in generator and async parameters, duplicate arrow and method parameters, duplicate catch
  bindings, `for (let x;;) { var x }`, `eval` or `arguments` as names of strict functions and
  generator or async functions as `if` bodies.
- Destructuring assignment evaluates target references before stepping iterators or reading
  properties, closes iterators when a generator is returned from inside a pattern and excludes
  computed keys from object rest.
- `super` property references evaluate their base before the key, throw a `TypeError` on a null
  base, support `++` and compound assignment, and a second `super()` call constructs the parent
  before throwing.
- `Array.fromAsync` follows the specification, the `RegExp` constructor honours `Symbol.match`
  objects, `RegExp.prototype.toString` reads `source` and `flags`, match results carry `indices`
  under the `d` flag, forward references and Annex B `\k` escapes compile, and the lexer
  re-tokenizes `/` as division or a regular expression from parser context.
- Class heritage is checked with IsConstructor and the `prototype` getter, bound and proxied
  functions work as super constructors, `"prototype" in method` is `false`, accessors from
  computed keys and functions in private fields get names, and functions list `length`, `name`
  and `prototype` first in `Object.getOwnPropertyNames`.
- `for await` closes its async iterator when the loop exits through `break`, `continue`,
  `return` or an exception, awaiting the result of `return()` and ignoring its failures after a
  throw; `delete super.x` throws a `ReferenceError` after checking `this`; and Annex B regular
  expression forms compile: quantified lookaheads, identity escapes such as `\P` and class
  escapes followed by `-`.

### Fixed

- JSON: the reviver deletes and defines through the ordinary operations, array holders are read
  with `Get`, replacer arrays accept `String` and `Number` objects and coerce the `space`
  argument, and getters that throw propagate.
- Object spread and rest read proxy traps in specification order; assignments to own accessor
  properties call their setters; `Object.prototype.__proto__` is immutable; `padStart` and
  `padEnd` report length 1; `String.prototype[Symbol.iterator]` rejects `null`; years before 0
  format with four digits; `Promise` rejects a second initialization and defines result slots
  with define semantics.
- Array `includes` and `indexOf` check the length before coercing `fromIndex`, `fill` and
  `reverse` only throw on frozen arrays when they would write, and `Array.prototype.toString`
  falls back correctly when `join` is not callable.
- `String.prototype.lastIndexOf` treats a `NaN` position as the end of the string, defining an
  array `length` converts the value with ToUint32 and ToNumber before any other check and
  reports mismatches as a `RangeError`, and `Date` constructed from local components clips the
  resulting time value.
- Compilation speed is unchanged from 0.9.0: scope lookups for `with` and `eval` only run when
  such scopes exist, and rarely used interpreter operations live outside the main dispatch loop.

## [0.9.0] - 2026-10-22

### Added

- `SharedArrayBuffer` with `grow`, `Atomics`, `Float16Array` and float16 access on `DataView`.
- Direct `eval` sees the locals, `this`, `new.target`, `super` and private names of the calling
  code and returns the completion value of its last statement; indirect `eval` runs as a script.
- Scripts and `Function` bodies parse in sloppy mode: duplicate parameters, `eval` and
  `arguments` as names and identifiers such as `static` are accepted until a `"use strict"`
  directive or a class body switches to strict mode.
- Module linking: importing a name a module does not export, a name two `export *` declarations
  provide, or a re-export that cycles back to itself is a `SyntaxError` before any module runs, and
  `import()` rejects with the same error. Re-exports are live bindings, `export * as default from`
  works, export names may be strings such as `export { x as "☿" }`, and `export default` accepts
  anonymous and `async` functions and generators as hoisted declarations.
- Imported `let`, `const` and `class` bindings throw a `ReferenceError` when read before the
  exporting module initializes them, including in cycles.
- `Proxy` enforces every invariant of its traps, proxies work as prototypes,
  `Object.prototype.toString` sees through proxies to arrays and `Function.prototype.apply` reads
  array-likes.
- Derived class constructors: `this` is in a temporal dead zone until `super()` runs, calling
  `super()` twice or returning a non-object is an error, `class extends null` cannot be
  constructed, and the class name is a constant binding inside the class body.
- Private brand checks: private methods and accessors are a `TypeError` on objects of another
  class, and accessor or read-only array elements can be defined with `Object.defineProperty`.
- `%ArrayIteratorPrototype%`, `%MapIteratorPrototype%`, `%SetIteratorPrototype%` and
  `%StringIteratorPrototype%` with their `toStringTag`s, `Array.prototype[Symbol.unscopables]`,
  and `keys`, `values` and `entries` on array-likes.
- Destructuring and `for...of` call `return()` on the iterator when a target or the loop body
  throws, and skip it when the iterator's own `next()` threw.
- The iterator helpers follow the specification's closing semantics, and `Iterator.prototype`
  gains the proposed `includes`, `chunks`, `windows` and `join`.
- `escape`, `unescape`, `RegExp.prototype.compile`, HTML-like comments in scripts and the Annex B
  HTML string methods such as `anchor` and `bold`.
- `Date.prototype[Symbol.toPrimitive]`, `Object.fromEntries` closing its iterator,
  `Object.assign` on primitives, `Function.prototype.bind` with the specified `length` and
  `name`, and `Set` and `Map` constructors that call an overridden `add` or `set`.

### Changed

- `Function.prototype` is itself callable and returns `undefined`.
- `Math.round`, `Math.hypot`, `toExponential` and `toPrecision` follow the specification for
  ties, `NaN` with `Infinity`, negative zero and omitted precision.
- `Number` and `Math` constants and the global `NaN`, `Infinity` and `undefined` are read-only
  and non-configurable, and `Number.parseFloat` and `Number.parseInt` are the global functions.
- Array `length` cannot shrink past a non-configurable element, a dense array may grow by at most
  50,000,000 elements at once, and indices at or above 2^32 - 1 are plain properties.
- `obj[key] op= value` converts `key` to a property key once, and reading or writing an index on
  `null` or `undefined` throws before the key is converted.
- JavaSquirt 0.9.0 passes 95% of the test262 strict mode tests it runs.

### Fixed

- `lastIndexOf` honours `fromIndex`, `concat` honours `Symbol.isConcatSpreadable`, `reduce`,
  `forEach` and the other callbacks skip elements deleted during iteration, and `Array.prototype`
  is itself an array.
- `Error.prototype.toString`, `Object.prototype.valueOf`, `isPrototypeOf`, `__lookupGetter__`
  and `propertyIsEnumerable` coerce `this` as specified.
- Static methods and accessors print without the `static` keyword in `toString`, and builtin
  functions written in JavaScript print as native code.

## [0.8.0] - 2026-10-22

### Added

- `BigInt` with literals, arithmetic, comparisons, `BigInt.asIntN`, `BigInt.asUintN` and
  `toString(radix)`, plus `BigInt64Array`, `BigUint64Array` and `getBigInt64`/`setBigInt64` on
  `DataView`.
- Primitive wrapper objects: `new String`, `new Number`, `new Boolean` and `Object(primitive)`,
  with `[String: 'a']` style output in `console.log`.
- The temporal dead zone: `let`, `const`, `class` and parameters throw a `ReferenceError` when
  used before initialization, and parameters bind left to right.
- Resizable `ArrayBuffer` with `resize` and `maxByteLength`, length-tracking typed arrays,
  `transfer`, `transferToFixedLength` and detached buffers.
- `%TypedArray%` as the shared parent of all typed array constructors, with `Symbol.species`
  support in `map`, `filter`, `slice` and `subarray`.
- Generic `Array.prototype` methods that work on any array-like object, `Symbol.species` for
  arrays, and subclassing of `Array`, `Error`, `Map`, `Promise`, `RegExp` and the other builtins.
- `RegExp.prototype[Symbol.match]`, `[Symbol.matchAll]`, `[Symbol.replace]`, `[Symbol.search]`
  and `[Symbol.split]`, and string methods that delegate to them, so custom matchers and
  `RegExp` subclasses work.
- `super[expr]`, assignment through `super.x` and `super[expr]`, and `export * as ns from`.
- `yield*` that forwards `next`, `throw` and `return` to the inner iterator and evaluates to its
  return value, in both sync and async generators.
- `GeneratorFunction`, `AsyncFunction` and `AsyncGeneratorFunction` constructors, and real
  `arguments` objects.
- Unicode escapes in identifiers, iterator helpers (`map`, `filter`, `take`, `drop`, `flatMap`,
  `reduce`, `toArray`, `Iterator.from`), `Set` methods (`union`, `intersection` and the rest),
  `Object.groupBy`, `Map.groupBy`, `Array.fromAsync`, `Promise.try`, `FinalizationRegistry`,
  `Error.isError`, `RegExp.escape`, `Math.f16round`, `String.prototype.isWellFormed` and
  `toWellFormed`, and `Intl` for numbers, plural rules, lists, relative times, collation and
  segmentation.

### Changed

- Strings follow UTF-16 semantics: lone surrogates are real values, pairs join when
  concatenated, and output to terminals and files uses U+FFFD for unpaired halves.
- `Promise.all`, `allSettled`, `any`, `race`, `resolve`, `reject`, `then` and `finally` follow
  the specification, including thenable jobs, so callbacks run in the same order as in Node.js.
- Generator parameters are evaluated when the generator is called, not on the first `next()`.
- Private names resolve to the nearest class that declares them, and undeclared ones are a
  `SyntaxError`.
- Early errors: duplicate lexical declarations, legacy octal literals, misplaced numeric
  separators, duplicate or undeclared exports, `new import()`, assignment to `eval` or
  `arguments`, and invalid class members such as `#constructor` or a static `prototype`.

### Fixed

- Array destructuring and spread honour a patched `Array.prototype[Symbol.iterator]`.
- Object patterns throw a `TypeError` on `null` and `undefined`, including empty patterns.
- Property stores are faster after startup, because builtins no longer disable the fast path.

## [0.7.0] - 2026-10-21

### Added

- Property attributes: `writable`, `enumerable` and `configurable` on every property, full
  `Object.defineProperty` validation, `Object.seal`, `Object.preventExtensions`,
  `Object.isSealed`, `Object.isExtensible`, `Object.getOwnPropertyDescriptors` and
  `Object.getOwnPropertySymbols`. Writes to read-only properties and to non-extensible objects
  throw a `TypeError`.
- `globalThis` as a real global object, including accessors defined on it.
- `Proxy` with all thirteen traps, `Proxy.revocable`, and `Reflect` with all thirteen functions.
- Well-known symbols `Symbol.toPrimitive`, `Symbol.hasInstance`, `Symbol.toStringTag`,
  `Symbol.species`, `Symbol.isConcatSpreadable`, `Symbol.unscopables`, `Symbol.match`,
  `Symbol.matchAll`, `Symbol.replace`, `Symbol.search` and `Symbol.split`, with the first three
  wired into conversions, `instanceof`, `Object.prototype.toString` and `console.log`.
- `var` with function scope and hoisting.
- Static initialization blocks, `this` in static field initializers, `new.target`, `super` in
  object literal methods, `__proto__` in object literals, and the inner binding of named class
  expressions.
- `new Function(...)`, `eval` (in the global scope), and `Function.prototype.toString` returning
  the source text.
- Modules: cycles, live bindings, `import()`, `import.meta` with `url`, `filename`, `dirname` and
  `resolve`, and `[Module: null prototype]` namespace objects.
- `generator.return()` runs `finally` blocks, and `for...of` loops and array destructuring close
  their iterators when they exit early.
- Array holes: `[1, , 3]`, `new Array(n)`, `delete arr[i]` and growing `length` leave real holes
  that `in`, `Object.keys`, `forEach`, `map`, `filter`, `indexOf`, `sort` and `console.log`
  respect.
- Cached and frozen template objects for tagged templates.
- `AggregateError`, `Error.captureStackTrace`, and `[cause]`, `[errors]` and nested stacks in
  `console.log` of errors.
- Function names for computed keys and accessors (`get x`, `[Symbol.iterator]`).

### Changed

- Error `message`, `stack` and `cause` are non-enumerable, as in JavaScript.
- Symbol identity no longer depends on the `Symbol` wrapper object.

## [0.6.0] - 2026-10-21

### Added

- Binary data: `ArrayBuffer`, `DataView` and the nine typed arrays (`Int8Array` through
  `Float64Array`, including `Uint8ClampedArray`) with their array methods, `from`, `of`,
  `subarray`, `set`, iteration and subclassing.
- `TextEncoder`, `TextDecoder`, `atob`, `btoa`, `Uint8Array.prototype.toHex` and `toBase64`,
  `Uint8Array.fromHex` and `fromBase64`.
- `structuredClone`, `crypto.getRandomValues` and `crypto.randomUUID`.
- `EventTarget`, `Event`, `AbortController`, `AbortSignal` (`abort`, `timeout`, `any`) and
  `DOMException`.
- Binary bodies everywhere: `arrayBuffer()` and `bytes()` on `Request` and `Response`, typed
  array and `ArrayBuffer` bodies for `fetch`, `serve` and `createServer`.
- `fetch` cancellation through `signal`.
- Node.js core modules, with or without the `node:` prefix: `fs` (sync, callback and promise
  APIs with Node error codes), `fs/promises`, `path`, `os`, `child_process`, `events`,
  `buffer`, `assert`, `util`, `url`, `readline`, `readline/promises`, `http`, `timers`,
  `timers/promises` and `process`.
- `Buffer` with the common encodings and the integer and float accessors.
- `process.stdin` with `data`/`end` events and `for await`, `process.stdout.write`,
  `process.stderr.write`, `process.nextTick`, `process.hrtime`, `process.uptime`,
  `process.pid`, `process.arch`, `process.versions`, `process.chdir` and `process.exitCode`.
- The `arguments` object.

### Fixed

- `super()` in a class that extends a builtin now binds `this` to the object the builtin
  constructor returns.
- Timers created by `AbortSignal.timeout` no longer keep the program running.

## [0.5.0] - 2026-10-21

### Added

- `fetch` over HTTP and HTTPS with redirects, timeouts and `TypeError: fetch failed` errors that
  carry a `cause`, running on background threads that feed the event loop.
- `URL`, `URLSearchParams`, `Headers`, `Request` and `Response`, loaded the first time a program
  uses them.
- `squirt:http` with `serve(options, handler)` for `Request` to `Response` handlers and a
  Node-style `createServer` with `req`/`res` objects. HTTP/1.1 keep-alive and chunked request
  bodies are supported.
- `jsq test` and `vrotdaem test`: discover `*.test.jsq` files, run each in a fresh interpreter,
  filter with `--filter`, and exit non-zero on failure.
- `squirt:test` with `describe`, `test`, `it`, `skip`, `only`, `todo`, `beforeEach`,
  `afterEach`, `expect` with 22 matchers plus `.not`, `.resolves` and `.rejects`, and `assert`.
- `squirt:path` (`posix` and `win32`), `squirt:os`, `squirt:child_process` (`execSync`,
  `spawnSync`) and `squirt:util` (`inspect`, `format`, `isDeepStrictEqual`).
- Tagged templates and `String.raw`.
- Non-enumerable properties: class members, builtin methods and properties defined with
  `Object.defineProperty` without `enumerable: true` are skipped by `Object.keys`, `for...in`,
  spread, `JSON.stringify` and `console.log`. `Object.defineProperties`,
  `propertyIsEnumerable` and descriptors that report `enumerable`.
- `for...in` visits enumerable inherited properties.
- `encodeURIComponent`, `encodeURI`, `decodeURIComponent`, `decodeURI`, `URIError` and
  `EvalError`.

### Changed

- `console.log` formats values exactly like Node's `util.inspect`: line breaking at 80 columns,
  grouped numeric arrays, split long strings and `<ref *1>` markers for cycles.
- Stack traces no longer show frames from builtin modules.

## [0.4.0] - 2026-10-21

### Added

- `Date` with local time zones and daylight saving time, `Date.now`, `Date.parse`, `Date.UTC`,
  every getter and setter in local and UTC flavors, `toISOString`, `toJSON`, `toString`,
  `toDateString`, `toTimeString`, `toUTCString` and the `toLocale*` formatters.
- `WeakMap`, `WeakSet` and `WeakRef`.
- `for await...of` over async iterables, sync iterables and promises.
- Async generators: `async function*`, async generator methods, `await` and `yield` together,
  `yield*` delegation to async iterables, and queued `next()` calls.
- More benchmarks: property access and method calls, arrays and strings.

### Changed

- Faster VM: comparisons fused with conditional jumps, in-place arithmetic, dedicated
  increment and store instructions, inline caches for global variables, a reusable argument
  buffer for native calls, and objects half their previous size with a compact property table.
- The `jsq` and `vrotdaem` binaries use the mimalloc allocator.
- Benchmarks compared to 0.3.0: object creation 2.2x faster, loops 1.8x, property access and
  method calls 1.35x, string processing 1.4x.

## [0.3.0] - 2026-10-21

### Added

- Regular expressions: `/pattern/flags` literals and `new RegExp(pattern, flags)` with the `d`,
  `g`, `i`, `m`, `s`, `u`, `v` and `y` flags, capture groups, named groups, backreferences,
  lookahead and lookbehind.
- `RegExp.prototype.exec`, `test`, `toString`, `lastIndex`, `source`, `flags` and the flag
  properties.
- Regex-aware `String.prototype.match`, `matchAll`, `search`, `replace`, `replaceAll` and `split`,
  including `$1`, `$<name>`, `$&`, `` $` ``, `$'` and `$$` in replacement strings and replacer
  functions.
- Private class members: `#fields`, `#methods`, static private members, private accessors and
  `#field in obj` brand checks.
- A cycle collector. Unreachable cycles of objects, closures, classes, maps and generators are
  reclaimed automatically; `JSQ_GC_THRESHOLD` tunes how often it runs.

### Fixed

- Programs that build cyclic structures in a loop no longer grow memory without bound.

## [0.2.0] - 2026-10-21

### Added

- `async` functions, async arrow functions and async methods with `await`, including top-level
  `await` in modules and in `jsq -e`.
- `Promise` with `then`, `catch`, `finally`, `Promise.resolve`, `Promise.reject`, `Promise.all`,
  `Promise.allSettled`, `Promise.race`, `Promise.any` and `Promise.withResolvers`.
- An event loop with a microtask queue: `setTimeout`, `setInterval`, `clearTimeout`,
  `clearInterval`, `queueMicrotask` and a `sleep(ms)` helper that returns a promise.
- Unhandled promise rejections fail the process with `Uncaught (in promise)`.
- Generators: `function*`, generator methods, `yield`, `yield*` delegation, and `next`, `return`
  and `throw` on generator objects.
- `Symbol` with `Symbol.iterator`, `Symbol.asyncIterator`, `Symbol.for`, `Symbol.keyFor` and
  `description`; symbols work as property keys and `Map` keys.
- The iterator protocol: any object with a `[Symbol.iterator]()` method works with `for...of`,
  spread, destructuring and `Array.from`.
- Getters and setters in classes (including `static`) and object literals, `super` access in
  accessors, and `get`/`set` in `Object.defineProperty`.
- `Object.getOwnPropertyDescriptor`.
- Labeled statements with `break label` and `continue label`, including labeled blocks.
- Interpreter coroutines: suspended frames keep their stack, handlers and captured variables.

### Changed

- Iterators returned by builtins expose `[Symbol.iterator]()`.
- `console.log` shows getters as `[Getter]`, `[Setter]` or `[Getter/Setter]`, promises with their
  state, and symbol keys after string keys.

## [0.1.0] - 2026-10-21

### Added

- `jsq` runtime with a REPL, file execution with arguments, `-e` snippet evaluation,
  `--disasm` bytecode dump and `--version`.
- `vrotdaem` package manager with `init`, `add`, `remove`, `gluboko` (with `--force` reinstall)
  and `run`, driven by `squirt.json` and installing into `nude_modules/`.
- Dependency specs `file:<path>` and `git:<url>[#ref]`.
- Compilation pipeline: lexer, Pratt parser with JavaScript-style automatic semicolon insertion,
  single-pass compiler to bytecode, and a stack VM with closures, upvalues and prototype-based objects.
- Value types: `undefined`, `null`, boolean, number, string, array, object, function, class,
  `Map`, `Set` and `Error`.
- Block-scoped `let` and `const`, hoisted function declarations, arrow functions, default and
  rest parameters.
- Spread in calls, array literals and object literals.
- Object and array destructuring with defaults and rest in `let`, `const` and parameters.
- Control flow: `if`/`else`, `while`, `do`/`while`, `for`, `for...of`, `for...in`, `switch`,
  `break` and `continue`.
- Exceptions: `try`/`catch`/`finally`, `throw`, and `Error`, `TypeError`, `RangeError`,
  `ReferenceError` and `SyntaxError` with stack traces.
- Classes with constructors, methods, static methods, instance and static fields, `extends`,
  `super()`, `super.method()`, `new` and `instanceof`.
- Operators: arithmetic and `**`, bitwise and shift operators including `>>>`, relational,
  loose and strict equality, `&&`, `||`, `??`, optional chaining, ternary, `typeof`, `void`,
  `delete`, `in`, compound assignment including `&&=`, `||=` and `??=`, `++` and `--`.
- Template literals, optional semicolons, line and block comments.
- ES modules: named, default and namespace imports, renamed imports and exports,
  `export const`/`let`/`function`/`class` and `export default`.
- Module resolution for relative paths (optional `.jsq` extension, `index.jsq` in directories),
  bare names from `nude_modules/` and the builtin `squirt:fs` module (`readFile`, `writeFile`,
  `appendFile`, `exists`, `remove`).
- Builtins: `console.log`/`info`/`warn`/`error`, `Math`, `Number`, `String`, `Array`, `Object`,
  `JSON.stringify`/`parse`, `Map`, `Set`, `parseInt`, `parseFloat`, `isNaN`, `isFinite`,
  `process.argv`, `process.exit` and `process.env`.
- Language reference in `docs/LANGUAGE.md`.
- Continuous integration on Linux, macOS and Windows, MSRV check, cargo-deny, CodeQL, coverage,
  benchmarks, and release archives for five targets.

[Unreleased]: https://github.com/00000kkkkk/javasquirt/compare/v1.5.0...HEAD
[1.5.0]: https://github.com/00000kkkkk/javasquirt/compare/v1.4.0...v1.5.0
[1.4.0]: https://github.com/00000kkkkk/javasquirt/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/00000kkkkk/javasquirt/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/00000kkkkk/javasquirt/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/00000kkkkk/javasquirt/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.10.0...v1.0.0
[0.10.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/00000kkkkk/javasquirt/releases/tag/v0.1.0
