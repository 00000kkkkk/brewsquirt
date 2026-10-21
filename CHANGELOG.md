# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/00000kkkkk/javasquirt/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/00000kkkkk/javasquirt/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/00000kkkkk/javasquirt/releases/tag/v0.1.0
