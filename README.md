# JavaSquirt

[![CI](https://github.com/00000kkkkk/javasquirt/actions/workflows/ci.yml/badge.svg)](https://github.com/00000kkkkk/javasquirt/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/)
[![Rust edition](https://img.shields.io/badge/edition-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/)

> High-performance language with JS semantics and Rust speed.
> Type `vrotdaem gluboko --force` to inject dependencies directly into your `nude_modules`.

JavaSquirt is a small, fast language that looks and behaves like the JavaScript you already know,
compiled to bytecode and executed by a stack VM written in Rust. It ships as two binaries:

- **`jsq`**: the runtime. Runs `.jsq` files, evaluates one-liners, dumps bytecode, and gives you a REPL.
- **`vrotdaem`**: the package manager. Manages `squirt.json` and installs dependencies into `nude_modules/`.

No `node_modules` black hole, no 400 MB of transitive left-pads. Just your code and whatever you
explicitly squirt into `nude_modules`.

## Quick example

```js
class Runtime {
  constructor(name, speed = 1) {
    this.name = name
    this.speed = speed
  }

  describe() {
    return `${this.name} goes ${this.speed}x`
  }
}

class Squirt extends Runtime {
  constructor() {
    super("JavaSquirt", 9000)
  }

  describe() {
    return `${super.describe()} and never stops`
  }
}

const runtimes = [new Runtime("interpreter"), new Runtime("tree-walker", 3), new Squirt()]
const fast = runtimes.filter((r) => r.speed > 1)
const lines = fast.map((r) => r.describe())
const total = runtimes.reduce((sum, r) => sum + r.speed, 0)

lines.forEach((line) => console.log(line))
console.log(`combined speed: ${total}x`)
```

```console
$ jsq speed.jsq
tree-walker goes 3x
JavaSquirt goes 9000x and never stops
combined speed: 9004x
```

## Install

No Rust toolchain needed. Every method below installs two binaries, `jsq` and `vrotdaem`.

### One-liner

Linux and macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/00000kkkkk/javasquirt/main/install.sh | sh
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/00000kkkkk/javasquirt/main/install.ps1 | iex
```

The scripts download the latest release for your platform, verify its SHA-256 checksum, install
into `~/.jsq/bin` (`%USERPROFILE%\.jsq\bin` on Windows) and add that directory to your `PATH`.
Set `JSQ_VERSION=v1.0.0` to pin a version, `JSQ_INSTALL_DIR` to pick another directory and
`JSQ_NO_MODIFY_PATH=1` to leave your shell profile alone.

### Package managers

Scoop (Windows):

```powershell
scoop bucket add javasquirt https://github.com/00000kkkkk/javasquirt
scoop install jsq
```

Homebrew (macOS and Linux):

```sh
brew tap 00000kkkkk/javasquirt https://github.com/00000kkkkk/javasquirt
brew install jsq
```

### From source

Requires Rust 1.85 or newer.

```sh
cargo install --git https://github.com/00000kkkkk/javasquirt
```

This installs both `jsq` and `vrotdaem` into `~/.cargo/bin`.

### Release binaries

Prebuilt archives for every tagged release are on the
[Releases page](https://github.com/00000kkkkk/javasquirt/releases):

| Platform            | Archive                                              |
| ------------------- | ---------------------------------------------------- |
| Linux x86_64        | `javasquirt-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`  |
| Linux ARM64         | `javasquirt-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz` |
| macOS Intel         | `javasquirt-vX.Y.Z-x86_64-apple-darwin.tar.gz`       |
| macOS Apple Silicon | `javasquirt-vX.Y.Z-aarch64-apple-darwin.tar.gz`      |
| Windows x86_64      | `javasquirt-vX.Y.Z-x86_64-pc-windows-msvc.zip`       |

Each archive has a matching `.sha256` checksum file. Unpack it and put `jsq` and `vrotdaem` on your `PATH`.

## Usage

### `jsq`: the runtime

| Command                         | What it does                                          |
| ------------------------------- | ----------------------------------------------------- |
| `jsq`                           | Start the REPL                                        |
| `jsq app.jsq arg1 arg2`         | Run a file; extra arguments land in `process.argv`    |
| `jsq -e "console.log(2 ** 10)"` | Evaluate a snippet                                    |
| `jsq -p "2 ** 10"`              | Evaluate a snippet and print its value                |
| `jsq --disasm app.jsq`          | Dump the compiled bytecode instead of running it      |
| `jsq test`                      | Run every `*.test.jsq` file under the current directory |
| `jsq --version`                 | Print the version                                     |

### `vrotdaem`: the package manager

| Command                          | What it does                                                    |
| -------------------------------- | --------------------------------------------------------------- |
| `vrotdaem init [--name <n>]`     | Create `squirt.json` in the current directory                   |
| `vrotdaem add <name> [spec]`     | Add a dependency from npm (`lodash`, `lodash@^4`), or `npm:<name>@<range>`, `file:<path>`, `git:<url>[#ref]` |
| `vrotdaem remove <name>`         | Remove a dependency                                             |
| `vrotdaem gluboko [--force]`     | Install all dependencies into `nude_modules/`                   |
| `vrotdaem run [args...]`         | Run the manifest's `main` with the given arguments              |

```sh
vrotdaem init --name my-app
vrotdaem add lodash
vrotdaem add dayjs@^1.11
vrotdaem add utils file:../utils
vrotdaem add left-pad git:https://github.com/someone/left-pad#v1.0.0
vrotdaem gluboko
vrotdaem run
```

And the one command you came here for:

```sh
vrotdaem gluboko --force
```

`gluboko` installs every dependency from `squirt.json` into `nude_modules/`. With `--force` it wipes
`nude_modules/` first and reinstalls everything from scratch, deep and clean.

A manifest looks like this:

```json
{
  "name": "my-app",
  "version": "0.1.0",
  "main": "main.jsq",
  "dependencies": {
    "lodash": "npm:lodash@^4.17.21",
    "utils": "file:../utils",
    "left-pad": "git:https://github.com/someone/left-pad#v1.0.0"
  }
}
```

Installed packages are imported by bare name; npm packages written as CommonJS or ES modules
both work, and their own dependencies are installed too, nested when versions conflict:

```js
import _ from "lodash"
import { pad } from "left-pad"
import { readFile } from "squirt:fs"
import helpers from "./helpers"
```

## Features

JavaSquirt 1.4.0 implements modern JavaScript as specified, passing 98% of test262:

- **Values**: `undefined`, `null`, booleans, numbers, strings, arrays, objects, functions, classes, `Map`, `Set`, `Error`
- **Bindings**: block-scoped `let` and `const`, hoisted function declarations
- **Functions**: arrow functions, default parameters, rest parameters, `arguments`, closures
- **Spread and destructuring**: spread in calls, arrays and objects; object and array destructuring with defaults and rest in `let`, `const` and parameters
- **Control flow**: `if`/`else`, `while`, `do`/`while`, `for`, `for...of`, `for...in`, `switch`, `break`, `continue`
- **Errors**: `try`/`catch`/`finally`, `throw`, `Error`, `TypeError`, `RangeError`, `ReferenceError`, `SyntaxError`, `URIError`, `EvalError`, with stack traces
- **Classes**: constructors, methods, static methods, instance and static fields, `extends`, `super()`, `super.method()`, `new`, `instanceof`, private `#members`
- **Operators**: arithmetic, `**`, bitwise and shifts including `>>>`, loose and strict equality, `&&`, `||`, `??`, optional chaining `?.`, ternary, `typeof`, `void`, `delete`, `in`, compound assignment including `&&=`, `||=`, `??=`, `++` and `--`
- **Syntax**: template literals and tagged templates with `String.raw`, optional semicolons with JS-style ASI, `//` and `/* */` comments
- **Properties**: full property attributes, `Object.defineProperty`, `freeze`, `seal`, `preventExtensions`, `for...in` over inherited properties, array holes
- **Metaprogramming**: `Proxy` with every trap, `Reflect`, `globalThis`, well-known symbols, `new Function`, `eval`
- **Async**: `async`/`await` (including top-level `await`), `Promise` with `all`, `allSettled`, `race`, `any`, an event loop with microtasks, `setTimeout`, `setInterval`, `queueMicrotask` and `sleep`
- **Async iteration**: `for await...of`, async generators and `Symbol.asyncIterator`
- **Dates**: `Date` with local time zones, parsing, formatting, getters and setters
- **Generators and iterators**: `function*`, `yield`, `yield*`, `Symbol.iterator` and the iterator protocol for `for...of`, spread and destructuring
- **Accessors and labels**: `get`/`set` in classes and object literals, `break label` and `continue label`
- **Regular expressions**: `/pattern/flags` literals and `RegExp` with groups, named groups, backreferences, lookaround, Unicode property escapes and `v`-flag set operations, plus `match`, `matchAll`, `replace`, `split` and `search`
- **Private members**: `#fields`, `#methods`, static private members and `#field in obj`
- **Memory**: reference counting plus a cycle collector, so cyclic garbage is reclaimed
- **Modules**: ES module `import` and `export` (named, default, namespace, renamed), live bindings, cycles, `import()`, `import.meta`, relative and bare-name resolution
- **Standard library**: `squirt:fs`, `squirt:path`, `squirt:os`, `squirt:child_process`, `squirt:util`
- **Binary data**: `ArrayBuffer`, typed arrays, `DataView`, `TextEncoder`, `TextDecoder`, `Buffer`, base64 and hex, `structuredClone`, `crypto.randomUUID`
- **WebAssembly**: `WebAssembly.instantiate`, modules, instances, memories, tables, globals and JavaScript imports on the bundled `wasmi` interpreter
- **Node.js compatibility**: `node:fs` with file descriptors, `FileHandle` and `fs.watch`, `node:path`, `node:events`, `node:assert`, `node:readline`, `node:util`, `node:url`, `node:test`, WebCrypto `crypto.subtle` keys, Node-style timers with `ref`/`unref`, real TCP and TLS sockets and servers in `node:net` and `node:tls`, HTTP and HTTPS servers with `upgrade` for WebSocket libraries, asynchronous `child_process` with `fork` IPC, thread-based `worker_threads` with shared memory and transferable ports, and more, so many Node scripts run unchanged; `process.stdin`, `process.stdout.write`, `process.exitCode`, `process.on("exit")`
- **Cancellation**: `AbortController`, `AbortSignal.timeout`, `EventTarget` and `DOMException`
- **Networking**: `fetch` over HTTP and HTTPS, `WebSocket`, TCP and TLS sockets, `URL`, `URLSearchParams`, `Headers`, `Request`, `Response`, and HTTP servers with `squirt:http` (`serve` and Node-style `createServer`)
- **Testing**: `jsq test` with `squirt:test` (`describe`, `test`, `expect`, `assert`, hooks, async tests)
- **Console**: output formatted exactly like Node's `util.inspect`
- **Builtins**: `console`, `Math`, `Number`, `String`, `Array`, `Object`, `JSON`, `Map`, `Set`, `WeakMap`, `WeakSet`, `WeakRef`, `Symbol`, `Promise`, `Date`, `parseInt`, `parseFloat`, `isNaN`, `isFinite`, `encodeURIComponent`, `decodeURIComponent`, `process.argv`, `process.exit`, `process.env`

The full reference with examples lives in [docs/LANGUAGE.md](docs/LANGUAGE.md).

## Architecture

```text
  source.jsq
      |
      v
 +-----------+  tokens  +----------------------+   AST   +-----------------------+
 |   Lexer   | -------> | Pratt parser         | ------> | Single-pass compiler  |
 |           |          | + JS-style ASI       |         | scopes, upvalues      |
 +-----------+          +----------------------+         +-----------------------+
                                                                    |
                                                                    | bytecode
                                                                    v
 +-------------------------------------------------------------------------------+
 |                                  Stack VM                                     |
 |   call frames | closures + upvalues | prototype chains | exception unwinding   |
 +-------------------------------------------------------------------------------+
          ^                                                       ^
          |                                                       |
 +--------------------------------+          +-------------------------------------+
 | Builtins                       |          | Module loader                       |
 | console, Math, JSON, Map, Set, |          | ./relative, nude_modules/<name>,    |
 | Error, Object, Array, process  |          | squirt:fs                           |
 +--------------------------------+          +-------------------------------------+
```

1. The **lexer** turns source text into tokens and records line breaks for ASI.
2. The **Pratt parser** builds the AST, inserting semicolons the way JavaScript does.
3. The **compiler** walks the AST once, resolves locals and upvalues, and emits compact bytecode.
4. The **VM** executes bytecode on a value stack with closures, prototype-based objects and
   `try`/`catch` unwinding. `jsq --disasm` shows exactly what it runs.

## Project layout

```text
javasquirt/
  src/
    lexer.rs, token.rs        tokens and the lexer
    parser.rs, ast.rs         Pratt parser and syntax tree
    compiler.rs, bytecode.rs  single-pass compiler and instruction set
    vm.rs, value.rs, object.rs  stack VM, values, objects and prototypes
    builtins/                 console, Math, JSON, Map, Set, Error and friends
    builtins/web.rs           fetch, URL, Request, Response, events, abort and squirt:http
    builtins/binary.rs        ArrayBuffer, typed arrays, DataView, text codecs, crypto
    builtins/node.rs          node: modules, Buffer, EventEmitter, readline, stdin
    builtins/testing.rs       squirt:test and squirt:util
    io.rs                     HTTP client and server threads feeding the event loop
    event_loop.rs             microtasks, timers, promises and I/O events
    runner.rs                 jsq test
    module.rs                 module resolution and loading
    pm/                       vrotdaem internals
    bin/jsq.rs                runtime CLI and REPL
    bin/vrotdaem.rs           package manager CLI
  tests/
    scripts/                  <name>.jsq programs with <name>.out expected stdout
    testing/                  fixtures for jsq test
  examples/                   runnable .jsq examples, executed in CI
  benches/                    .jsq benchmarks, timed with hyperfine in CI
  docs/LANGUAGE.md            language reference
```

## Development

```sh
cargo build
cargo test --all-features
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo run --bin jsq -- benches/fib.jsq
cargo run --release --bin jsq -- --disasm benches/loop.jsq
```

Optional tools used in CI:

```sh
cargo deny check
cargo llvm-cov --all-features
typos
```

## Roadmap

- [x] `async`/`await`, Promises and an event loop
- [x] Generators
- [x] Getters and setters
- [x] Symbols and the iterator protocol
- [x] Labeled statements
- [x] Regular expressions
- [x] Private `#fields`
- [x] Cycle collector
- [x] Async generators and `for await`
- [x] `Date`
- [x] `fetch`, `URL` and HTTP servers
- [x] Test runner
- [x] `ArrayBuffer`, typed arrays and binary bodies
- [x] Node.js core modules
- [x] `BigInt`, `Proxy`, `Reflect` and `Intl`
- [x] `SharedArrayBuffer` and `Atomics`
- [x] 95% of test262
- [x] 97% of test262
- [x] 98% of test262
- [x] Mapped `arguments` and the global lexical environment
- [x] 1.0.0
- [x] Install without Rust: one-liners, Scoop and Homebrew
- [x] Node compatibility: streams, crypto, zlib, http client, CommonJS `require`
- [x] Web streams, Blob, File and FormData
- [x] WebAssembly
- [ ] 99% of test262
- [ ] Own regular expression engine with lookbehind back-references and the `v` flag
- [ ] WebSockets
- [ ] Rope strings for fast `+=` in loops
- [ ] Shapes and inline caches for property access
- [ ] JIT compiler

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) for the workflow, commit
conventions and how to add tests. Please follow the [Code of Conduct](CODE_OF_CONDUCT.md), and
report vulnerabilities as described in [SECURITY.md](SECURITY.md).

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
