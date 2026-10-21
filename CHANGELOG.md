# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/00000kkkkk/javasquirt/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/00000kkkkk/javasquirt/releases/tag/v0.1.0
