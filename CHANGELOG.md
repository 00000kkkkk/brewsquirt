# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- Globals `setImmediate`, `clearImmediate`, `global`, `navigator`, `reportError` and
  `CustomEvent`; `performance.mark`, `measure`, `timeOrigin` and entry queries;
  `process.availableParallelism`.
- Node modules `string_decoder`, `querystring`, `tty` and `perf_hooks`.
- `node:stream`: `Readable`, `Writable`, `Duplex`, `Transform`, `PassThrough`, `Readable.from`,
  `pipeline`, `finished` and the `stream/promises` versions, with backpressure, `drain`,
  object mode, encodings and async iteration.
- `http.request`, `http.get`, `http.Agent` and the `node:https` module: client requests built
  on `fetch` whose responses are readable streams with `statusCode`, `headers`, `setEncoding`
  and `json()`.

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

[Unreleased]: https://github.com/00000kkkkk/javasquirt/compare/v1.0.0...HEAD
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
