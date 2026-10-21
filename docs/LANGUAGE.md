# JavaSquirt Language Reference

This document describes JavaSquirt 0.4.0. JavaSquirt follows JavaScript semantics: for every
supported feature, a program should print the same output under `jsq` as under Node.js. Features
that are not implemented yet are listed in [Not yet supported](#not-yet-supported).

## Contents

- [Running programs](#running-programs)
- [Lexical structure](#lexical-structure)
- [Types and values](#types-and-values)
- [Variables and scope](#variables-and-scope)
- [Operators](#operators)
- [Functions](#functions)
- [Spread and destructuring](#spread-and-destructuring)
- [Control flow](#control-flow)
- [Errors and exceptions](#errors-and-exceptions)
- [Classes](#classes)
- [Private members](#private-members)
- [Getters and setters](#getters-and-setters)
- [Async functions and promises](#async-functions-and-promises)
- [Generators and iterators](#generators-and-iterators)
- [Async iteration](#async-iteration)
- [Dates](#dates)
- [Regular expressions](#regular-expressions)
- [Modules](#modules)
- [Builtins](#builtins)
- [Memory management](#memory-management)
- [CLI reference](#cli-reference)
- [Not yet supported](#not-yet-supported)

## Running programs

Source files use the `.jsq` extension and are UTF-8 text. A file is a module: its top level runs
once, from top to bottom.

```sh
jsq hello.jsq
```

```js
const who = "world"
console.log(`hello, ${who}`)
```

```text
hello, world
```

## Lexical structure

### Comments

Line comments start with `//` and run to the end of the line. Block comments start with `/*`,
end with `*/`, may span lines and do not nest. Both are ignored by the compiler.

### Semicolons and ASI

Semicolons are optional. The parser applies JavaScript automatic semicolon insertion: a statement
ends at a line break when the next token cannot continue it.

```js
const a = 1
const b = 2
console.log(a + b)
```

The usual JavaScript caveats apply. A line starting with `(`, `[` or a template literal continues
the previous expression, and a line break directly after `return`, `throw`, `break`, `continue`
or before a postfix `++`/`--` ends the statement.

```js
function broken() {
  return
    42
}
console.log(broken())
```

```text
undefined
```

### Identifiers and keywords

Identifiers start with a letter, `_` or `$`, followed by letters, digits, `_` or `$`. Reserved
words such as `let`, `const`, `function`, `class`, `return`, `if`, `for`, `import` and `export`
cannot be used as binding names.

### Literals

| Literal          | Examples                                    |
| ---------------- | ------------------------------------------- |
| Number           | `42`, `3.14`, `1e9`, `2.5e-3`               |
| String           | `"double"`, `'single'`, `"tab\tand\nnewline"` |
| Template literal | `` `sum: ${a + b}` ``                       |
| Boolean          | `true`, `false`                             |
| Null, undefined  | `null`, `undefined`                         |
| Array            | `[1, 2, 3]`, `[]`, `[...xs, 4]`             |
| Object           | `{ name: "jsq", "quoted-key": 1 }`          |
| Function         | `function (x) { return x }`, `(x) => x * 2` |
| Class            | `class Point { }`                           |

String escapes include `\n`, `\t`, `\r`, `\\`, `\"`, `\'` and `` \` ``.

### Template literals

Template literals are delimited by backticks, may span lines, and interpolate any expression
inside `${...}`. Interpolated values are converted with the same rules as `String(value)`.

```js
const items = ["lexer", "parser", "vm"]
console.log(`stages: ${items.length}
first: ${items[0].toUpperCase()}
done: ${items.includes("vm") ? "yes" : "no"}`)
```

```text
stages: 3
first: LEXER
done: yes
```

## Types and values

| Type        | `typeof`      | Notes                                                        |
| ----------- | ------------- | ------------------------------------------------------------ |
| `undefined` | `"undefined"` | Value of missing variables, properties and return values     |
| `null`      | `"object"`    | Intentional absence of a value                               |
| boolean     | `"boolean"`   | `true` and `false`                                           |
| number      | `"number"`    | IEEE 754 double, including `NaN`, `Infinity` and `-0`        |
| string      | `"string"`    | Immutable sequence of characters                             |
| array       | `"object"`    | Ordered, growable list with a `length` property              |
| object      | `"object"`    | String-keyed properties with a prototype                     |
| function    | `"function"`  | Functions, arrow functions, closures and methods             |
| class       | `"function"`  | Classes are constructor functions with a prototype           |
| `Map`       | `"object"`    | Key/value collection with insertion order                    |
| `Set`       | `"object"`    | Collection of unique values with insertion order             |
| `Error`     | `"object"`    | Error objects with `name`, `message` and `stack`             |

Semantics notes:

- Falsy values are `false`, `0`, `-0`, `NaN`, `""`, `null` and `undefined`. Everything else is
  truthy, including `[]` and `{}`.
- Primitives (`undefined`, `null`, booleans, numbers, strings) are compared by value. Arrays,
  objects, functions, `Map`, `Set` and errors are compared by reference.
- Integer arithmetic is exact up to `2 ** 53`. Division by zero produces `Infinity`, `-Infinity`
  or `NaN`, never an exception.
- Reading a missing property yields `undefined`. Reading a property of `undefined` or `null`
  throws a `TypeError`.
- Objects inherit through prototype chains: property lookup walks the chain until it finds the
  key or reaches `null`.

## Variables and scope

`let` declares a reassignable binding, `const` a binding that cannot be reassigned. Both are
block scoped.

```js
let count = 1
const limit = 3

if (count < limit) {
  let count = 10
  console.log(count)
}

console.log(count)
```

```text
10
1
```

Reassigning a `const` binding is an error. The object a `const` refers to can still be mutated:

```js
const config = { debug: false }
config.debug = true
console.log(config.debug)
```

Function declarations are hoisted to the top of their scope, so they can be called before they
appear:

```js
console.log(square(7))

function square(x) {
  return x * x
}
```

Reading an undeclared variable throws a `ReferenceError`.

## Operators

### Arithmetic

`+`, `-`, `*`, `/`, `%`, `**`, unary `+` and `-`. `+` concatenates when either operand is a
string. `%` takes the sign of the dividend. `**` is right associative.

```js
console.log(7 % 3, -7 % 3, 2 ** 3 ** 2, "1" + 2, "6" * "7")
```

```text
1 -1 512 12 42
```

### Bitwise

`&`, `|`, `^`, `~`, `<<`, `>>` and `>>>`. Operands are converted to 32-bit integers; `>>>`
produces an unsigned result.

```js
console.log(5 & 3, 5 | 3, 5 ^ 3, ~5, 1 << 10, -16 >> 2, -16 >>> 28)
```

```text
1 7 6 -6 1024 -4 15
```

### Comparison and equality

`<`, `>`, `<=`, `>=` compare numbers numerically and strings lexicographically. `===` and `!==`
compare without type conversion. `==` and `!=` apply JavaScript loose equality, including
`null == undefined`.

```js
console.log(1 === 1, "1" === 1, "1" == 1, null == undefined, null === undefined, NaN === NaN)
```

```text
true false true true false false
```

### Logical and nullish

`&&` and `||` short-circuit and return one of their operands. `??` returns the right operand only
when the left one is `null` or `undefined`.

```js
const port = 0
console.log(port || 8080, port ?? 8080, "" && "never", null ?? "fallback")
```

```text
8080 0  fallback
```

### Optional chaining

`?.` stops evaluation and returns `undefined` when the value on its left is `null` or
`undefined`. It works for property access, computed access and calls.

```js
const user = { profile: { name: "squirt" }, greet: null }
console.log(user.profile?.name, user.settings?.theme, user.list?.[0], user.greet?.())
```

```text
squirt undefined undefined undefined
```

### Conditional

```js
const n = 5
console.log(n % 2 === 0 ? "even" : "odd")
```

### Unary keyword operators

- `typeof x` returns the type name as a string. It does not throw for undeclared variables.
- `void expr` evaluates `expr` and returns `undefined`.
- `delete obj.key` removes an own property and returns `true`.
- `key in obj` checks whether a property exists on the object or its prototype chain.

```js
const point = { x: 1, y: 2 }
delete point.y
console.log(typeof point, typeof missing, void 0, "x" in point, "y" in point, 0 in [9])
```

```text
object undefined undefined true false true
```

### Assignment

`=`, `+=`, `-=`, `*=`, `/=`, `%=`, `**=`, `&=`, `|=`, `^=`, `<<=`, `>>=`, `>>>=`, and the
logical forms `&&=`, `||=`, `??=`, which only assign when the logical operator would evaluate its
right side.

```js
const opts = { retries: 0, name: "" }
opts.retries ||= 3
opts.name ??= "default"
opts.timeout ??= 1000
console.log(opts.retries, JSON.stringify(opts.name), opts.timeout)
```

```text
3 "" 1000
```

### Increment and decrement

Prefix `++x`/`--x` return the new value, postfix `x++`/`x--` return the old one.

```js
let i = 1
console.log(i++, i, ++i)
```

```text
1 2 3
```

### Precedence

From highest to lowest:

| Operators                                              | Associativity |
| ------------------------------------------------------ | ------------- |
| `.` `?.` `[]` `()` `new`                               | left          |
| postfix `++` `--`                                      |               |
| `!` `~` unary `+` `-` prefix `++` `--` `typeof` `void` `delete` | right |
| `**`                                                   | right         |
| `*` `/` `%`                                            | left          |
| `+` `-`                                                | left          |
| `<<` `>>` `>>>`                                        | left          |
| `<` `>` `<=` `>=` `in` `instanceof`                    | left          |
| `==` `!=` `===` `!==`                                  | left          |
| `&`                                                    | left          |
| `^`                                                    | left          |
| `\|`                                                   | left          |
| `&&`                                                   | left          |
| `\|\|` `??`                                            | left          |
| `? :`                                                  | right         |
| `=` and compound assignment                            | right         |

## Functions

### Declarations and expressions

```js
function add(a, b) {
  return a + b
}

const sub = function (a, b) {
  return a - b
}

console.log(add(2, 3), sub(2, 3))
```

A function without a `return` statement returns `undefined`. Missing arguments are `undefined`,
extra arguments are ignored.

### Arrow functions

Arrow functions have a concise body form and capture `this` from the surrounding scope.

```js
const double = (x) => x * 2
const sum = (a, b) => {
  const total = a + b
  return total
}
const makePoint = (x, y) => ({ x: x, y: y })

console.log(double(21), sum(1, 2), makePoint(1, 2).y)
```

### Default and rest parameters

Defaults are evaluated at call time when the argument is `undefined` and may refer to earlier
parameters. A rest parameter collects the remaining arguments into an array and must come last.

```js
function greet(name = "stranger", greeting = `hi ${name}`) {
  return greeting
}

function tally(label, ...values) {
  return `${label}: ${values.length}`
}

console.log(greet(), greet("jsq"), tally("args", 1, 2, 3))
```

```text
hi stranger hi jsq args: 3
```

### Closures

Functions capture the variables of their enclosing scopes by reference. Captured variables live
as long as any closure that uses them.

```js
function counter() {
  let n = 0
  return () => ++n
}

const next = counter()
next()
next()
console.log(next())
```

```text
3
```

### Methods and `this`

When a function is called as `obj.method()`, `this` is `obj`. Plain calls get `this` as
`undefined`.

```js
const account = {
  balance: 10,
  deposit: function (amount) {
    this.balance += amount
    return this.balance
  }
}

console.log(account.deposit(5))
```

## Spread and destructuring

### Spread

```js
const base = [1, 2]
const more = [...base, 3, ...[4, 5]]
const defaults = { color: "blue", size: 1 }
const custom = { ...defaults, size: 3 }

console.log(Math.max(...more), more.length, custom.color, custom.size)
```

```text
5 5 blue 3
```

Object spread copies own enumerable properties; later properties win.

### Destructuring

Destructuring works in `let`, `const` and function parameters, for objects and arrays, with
defaults, renaming, nesting and rest elements.

```js
const { name, version = "0.0.0", meta: { license } } = { name: "jsq", meta: { license: "Apache-2.0" } }
const [first, second = "none", ...others] = [1, undefined, 4, 5]
const { a: renamed, ...remaining } = { a: 1, b: 2, c: 3 }

function area({ width, height = width }) {
  return width * height
}

function head([x, ...tail]) {
  return `${x} then ${tail.length}`
}

console.log(name, version, license)
console.log(first, second, others)
console.log(renamed, Object.keys(remaining))
console.log(area({ width: 4 }), head([7, 8, 9]))
```

```text
jsq 0.0.0 Apache-2.0
1 none [ 4, 5 ]
1 [ 'b', 'c' ]
16 7 then 2
```

Defaults apply only when the value is `undefined`. Destructuring `null` or `undefined` throws a
`TypeError`.

## Control flow

### `if` / `else`

```js
const temp = 21
if (temp < 10) {
  console.log("cold")
} else if (temp < 25) {
  console.log("nice")
} else {
  console.log("hot")
}
```

### `while` and `do` / `while`

```js
let n = 3
while (n > 0) {
  n--
}

let tries = 0
do {
  tries++
} while (tries < 3)

console.log(n, tries)
```

The body of `do`/`while` always runs at least once.

### `for`

```js
let total = 0
for (let i = 0; i < 5; i++) {
  total += i
}
console.log(total)
```

All three clauses are optional; `for (;;)` loops until `break`.

### `for...of`

Iterates over values: elements of arrays, characters of strings, `[key, value]` pairs of a `Map`
and values of a `Set`.

```js
for (const ch of "jsq") {
  console.log(ch)
}

for (const [key, value] of new Map([["a", 1], ["b", 2]])) {
  console.log(key, value)
}
```

### `for...in`

Iterates over the enumerable string keys of an object. For arrays the keys are the indices as
strings.

```js
const scores = { ann: 3, bob: 5 }
for (const name in scores) {
  console.log(name, scores[name])
}
```

### `switch`

Cases are compared with `===`. Execution falls through to the next case until a `break`.
`default` runs when no case matches.

```js
function kind(x) {
  switch (typeof x) {
    case "number":
    case "boolean":
      return "scalar"
    case "string":
      return "text"
    default:
      return "other"
  }
}

console.log(kind(1), kind(true), kind("a"), kind(null))
```

### `break` and `continue`

`break` exits the innermost loop or `switch`. `continue` skips to the next iteration of the
innermost loop.

```js
const odd = []
for (let i = 0; i < 10; i++) {
  if (i % 2 === 0) continue
  if (i > 7) break
  odd.push(i)
}
console.log(odd.join(","))
```

```text
1,3,5,7
```

### Labeled statements

A label names a loop or a block. `break label` leaves the labeled statement, `continue label`
starts the next iteration of the labeled loop.

```js
outer: for (let i = 0; i < 3; i++) {
  for (let j = 0; j < 3; j++) {
    if (j === 1) continue outer
    if (i === 2) break outer
    console.log(i, j)
  }
}

check: {
  if (Date.now() > 0) break check
  console.log("never printed")
}
```

```text
0 0
1 0
```

## Errors and exceptions

### `throw`, `try`, `catch`, `finally`

Any value can be thrown. `catch` receives it; the binding is optional to use but required to
declare. `finally` always runs, whether the block completes, throws or returns.

```js
function parse(text) {
  try {
    return JSON.parse(text)
  } catch (err) {
    console.log(`bad input: ${err.name}`)
    return null
  } finally {
    console.log("parse attempted")
  }
}

parse("{")
```

```text
bad input: SyntaxError
parse attempted
```

### Error types

`Error`, `TypeError`, `RangeError`, `ReferenceError` and `SyntaxError` are available as
constructors. Each error has `name`, `message` and `stack`. The runtime throws them for its own
failures:

| Error            | Thrown when                                                         |
| ---------------- | ------------------------------------------------------------------- |
| `TypeError`      | Calling a non-function, reading a property of `null`/`undefined`, `new` on a non-constructor |
| `ReferenceError` | Reading an undeclared variable                                      |
| `RangeError`     | A numeric argument is outside its allowed range                     |
| `SyntaxError`    | Invalid source code or invalid JSON in `JSON.parse`                 |

```js
class ValidationError extends Error {
  constructor(field) {
    super(`invalid field: ${field}`)
    this.name = "ValidationError"
    this.field = field
  }
}

try {
  throw new ValidationError("email")
} catch (err) {
  console.log(err instanceof ValidationError, err instanceof Error, err.message)
}
```

```text
true true invalid field: email
```

### Stack traces

`err.stack` contains the error name, the message and the chain of active function calls with
their source locations. An uncaught exception prints the error and its stack trace to stderr
and makes `jsq` exit with a non-zero status.

## Classes

```js
class Shape {
  static count = 0
  sides = 0

  constructor(name) {
    this.name = name
    Shape.count++
  }

  describe() {
    return `${this.name} with ${this.sides} sides`
  }

  static total() {
    return `${Shape.count} shapes`
  }
}

class Square extends Shape {
  sides = 4

  constructor(size) {
    super("square")
    this.size = size
  }

  area() {
    return this.size ** 2
  }

  describe() {
    return `${super.describe()}, area ${this.area()}`
  }
}

const sq = new Square(3)
console.log(sq.describe())
console.log(sq instanceof Square, sq instanceof Shape, typeof Square)
console.log(Shape.total())
```

```text
square with 4 sides, area 9
true true function
1 shapes
```

Rules:

- `constructor` is optional. A derived class without one forwards its arguments to `super(...)`.
- In a derived constructor, `super(...)` must be called before `this` is used.
- Instance fields of a base class are initialized before its constructor body runs; fields of a
  derived class are initialized right after `super(...)` returns.
- Static fields and methods live on the class itself, not on instances.
- `super.method()` calls the parent prototype's method with the current `this`.
- Methods live on the prototype and are shared by all instances.
- `instanceof` checks whether the constructor's prototype is on the object's prototype chain.

## Private members

Names starting with `#` are private to the class body that declares them: fields, methods,
accessors and their `static` variants. They are invisible to `Object.keys`, `JSON.stringify`,
spread and `console.log`. `#name in obj` checks whether an object carries the private member.

```js
class Account {
  #balance = 0
  static #opened = 0

  constructor(owner) {
    this.owner = owner
    Account.#opened++
  }

  #log(msg) {
    return `${this.owner}: ${msg}`
  }

  deposit(n) {
    this.#balance += n
    return this.#log(`+${n}`)
  }

  get balance() {
    return this.#balance
  }

  static opened() {
    return Account.#opened
  }

  static isAccount(x) {
    return #balance in x
  }
}

const acc = new Account("ann")
console.log(acc.deposit(5), acc.balance, Account.opened())
console.log(acc, Account.isAccount(acc), Account.isAccount({}))
```

```text
ann: +5 5 1
Account { owner: 'ann' } true false
```

## Getters and setters

`get` and `set` define accessor properties in classes (instance and `static`) and in object
literals. Reading the property calls the getter, assigning calls the setter. A property with only
a getter throws a `TypeError` when assigned.

```js
class Temperature {
  constructor(c) {
    this.c = c
  }
  get fahrenheit() {
    return this.c * 1.8 + 32
  }
  set fahrenheit(f) {
    this.c = (f - 32) / 1.8
  }
}

const t = new Temperature(100)
t.fahrenheit = 212
console.log(t.fahrenheit, t.c)

const user = {
  first: "Java",
  last: "Squirt",
  get full() {
    return `${this.first} ${this.last}`
  },
}
console.log(user.full, user)
```

```text
212 100
Java Squirt { first: 'Java', last: 'Squirt', full: [Getter] }
```

`Object.defineProperty(obj, key, { get, set })` defines accessors at runtime, and
`super.prop` inside a class accessor reads the parent's accessor.

## Async functions and promises

`async` functions always return a `Promise`. Inside them, `await` suspends the function until the
awaited value settles, without blocking anything else. `await` also works at the top level of a
module.

```js
const wait = (ms, value) => new Promise((resolve) => setTimeout(() => resolve(value), ms))

async function load(id) {
  await wait(5)
  if (id < 0) throw new RangeError(`bad id ${id}`)
  return { id }
}

try {
  await load(-1)
} catch (e) {
  console.log(e.name)
}

const results = await Promise.all([load(1), load(2), 3])
console.log(results)
console.log(await Promise.race([wait(50, "slow"), wait(1, "fast")]))
```

```text
RangeError
[ { id: 1 }, { id: 2 }, 3 ]
fast
```

The event loop follows JavaScript ordering: synchronous code first, then every queued microtask
(promise reactions, `queueMicrotask`), then timers in order of their due time.

| API                                    | Description                                            |
| -------------------------------------- | ------------------------------------------------------ |
| `new Promise((resolve, reject) => {})` | Create a promise                                       |
| `p.then(onFulfilled, onRejected)`      | Chain a reaction                                       |
| `p.catch(onRejected)`, `p.finally(fn)` | Handle rejection, run cleanup                          |
| `Promise.resolve(v)`, `Promise.reject(e)` | Settled promises                                    |
| `Promise.all`, `allSettled`, `race`, `any` | Combine promises                                   |
| `Promise.withResolvers()`              | `{ promise, resolve, reject }`                         |
| `setTimeout(fn, ms, ...args)`          | Run once after `ms`, returns an id                     |
| `setInterval(fn, ms, ...args)`         | Run every `ms`, returns an id                          |
| `clearTimeout(id)`, `clearInterval(id)` | Cancel a timer                                        |
| `queueMicrotask(fn)`                   | Run `fn` after the current job                         |
| `sleep(ms)`                            | A promise that resolves after `ms`                     |

A rejected promise that nobody handles stops the program with `Uncaught (in promise)` and a
non-zero exit code.

## Generators and iterators

`function*` declares a generator. Calling it returns a generator object; each `next(value)` runs
the body until the next `yield`. `yield*` delegates to another iterable.

```js
function* count(from, to) {
  for (let i = from; i <= to; i++) yield i
}

function* all() {
  yield* count(1, 3)
  yield* ["a", "b"]
}

console.log([...all()])

const talk = (function* () {
  const name = yield "who?"
  return `hi ${name}`
})()
console.log(talk.next().value, talk.next("squirt"))
```

```text
[ 1, 2, 3, 'a', 'b' ]
who? { value: 'hi squirt', done: true }
```

Any object with a `[Symbol.iterator]()` method is iterable: `for...of`, spread, array
destructuring, `Array.from`, `new Map(...)` and `new Set(...)` all use the iterator protocol.

```js
class Range {
  constructor(from, to) {
    this.from = from
    this.to = to
  }
  *[Symbol.iterator]() {
    for (let i = this.from; i <= this.to; i++) yield i
  }
}
console.log(Math.max(...new Range(1, 9)))
```

```text
9
```

`Symbol(description)` creates a unique key. `Symbol.for(key)` returns a shared symbol from the
global registry. Symbol-keyed properties are skipped by `Object.keys`, `for...in` and
`JSON.stringify`.

## Async iteration

`for await...of` walks an async iterable, awaiting each step. It also accepts ordinary iterables,
awaiting each value, so an array of promises works. `async function*` declares an async
generator: it can `await` and `yield`, and every `next()` returns a promise. Calls to `next()`
made before the previous one settles are queued and answered in order.

```js
const wait = (ms, v) => new Promise((resolve) => setTimeout(() => resolve(v), ms))

async function* ticks(n) {
  for (let i = 1; i <= n; i++) {
    await wait(2)
    yield i
  }
}

async function* scaled(source, k) {
  for await (const x of source) yield x * k
}

const out = []
for await (const v of scaled(ticks(3), 10)) out.push(v)
console.log(out)

for await (const v of [wait(3, "a"), "b"]) console.log(v)

const gen = ticks(1)
console.log(await gen.next(), await gen.next())
```

```text
[ 10, 20, 30 ]
a
b
{ value: 1, done: false } { value: undefined, done: true }
```

Objects become async iterable by implementing `[Symbol.asyncIterator]()`, and classes can declare
`async *[Symbol.asyncIterator]() { ... }`.

## Dates

`Date` stores milliseconds since 1970-01-01T00:00:00Z. Local getters and setters use the
system time zone, including daylight saving time; the `UTC` variants ignore it.

```js
const d = new Date(Date.UTC(2026, 9, 21, 14, 5, 9))
console.log(d.toISOString(), d.getUTCDay(), d.getUTCHours())
console.log(JSON.stringify({ at: d }))

const due = new Date(d)
due.setUTCDate(due.getUTCDate() + 30)
console.log(due.toISOString(), (due - d) / 86400000)

console.log(Date.parse("2026-10-21T12:00:00Z"), new Date("nope").getTime())
console.log(new Date(Date.UTC(2024, 1, 29)).toUTCString())
```

```text
2026-10-21T14:05:09.000Z 3 14
{"at":"2026-10-21T14:05:09.000Z"}
2026-11-20T14:05:09.000Z 30
1792584000000 NaN
Thu, 29 Feb 2024 00:00:00 GMT
```

| Group                | Methods                                                                    |
| -------------------- | -------------------------------------------------------------------------- |
| Create               | `new Date()`, `new Date(ms)`, `new Date(string)`, `new Date(y, m, d, h, mi, s, ms)` |
| Static               | `Date.now()`, `Date.parse(string)`, `Date.UTC(y, m, ...)`                  |
| Local getters        | `getFullYear`, `getMonth`, `getDate`, `getDay`, `getHours`, `getMinutes`, `getSeconds`, `getMilliseconds`, `getTimezoneOffset` |
| UTC getters          | `getUTCFullYear` ... `getUTCMilliseconds`                                  |
| Setters              | `setTime` and every `set...`/`setUTC...` counterpart                       |
| Formatting           | `toISOString`, `toJSON`, `toString`, `toDateString`, `toTimeString`, `toUTCString`, `toLocaleString`, `toLocaleDateString`, `toLocaleTimeString` |

`Date.parse` understands ISO 8601 (date-only strings are UTC, date-time strings without an
offset are local), plus the formats produced by `toString` and `toUTCString`. Invalid dates have
a `NaN` time value and print as `Invalid Date`.

## Regular expressions

Regex literals `/pattern/flags` and `new RegExp(pattern, flags)` follow JavaScript syntax:
character classes, quantifiers (greedy and lazy), anchors, alternation, capture groups, named
groups `(?<name>...)`, backreferences `\1` and `\k<name>`, lookahead `(?=...)`, `(?!...)` and
lookbehind `(?<=...)`, `(?<!...)`. Flags: `g` global, `i` ignore case, `m` multiline, `s` dot
matches newlines, `u` and `v` unicode, `y` sticky, `d` indices. Positions such as `index` and
`lastIndex` count UTF-16 code units, exactly like JavaScript.

```js
const date = /(?<year>\d{4})-(?<month>\d{2})-(?<day>\d{2})/
const m = "shipped 2026-10-21".match(date)
console.log(m.index, m.groups.year, m[3])

console.log("a1b22c333".match(/\d+/g))
console.log("John Smith".replace(/(\w+) (\w+)/, "$2, $1"))
console.log("background-color".replace(/-(\w)/g, (_, c) => c.toUpperCase()))
console.log("a, b;c".split(/[,;]\s*/), "hello".search(/l+/))

const word = /\w+/g
let hit
while ((hit = word.exec("go deep")) !== null) console.log(hit[0], word.lastIndex)

console.log([..."x1y2".matchAll(/([a-z])(\d)/g)].map((r) => r[1] + r[2]))
```

```text
8 2026 21
[ '1', '22', '333' ]
Smith, John
backgroundColor
[ 'a', 'b', 'c' ] 2
go 2
deep 7
[ 'x1', 'y2' ]
```

| Method                              | Result                                                    |
| ----------------------------------- | --------------------------------------------------------- |
| `re.test(s)`                        | `true` if the pattern matches                             |
| `re.exec(s)`                        | A match array with `index`, `input` and `groups`, or `null` |
| `s.match(re)`                       | First match, or every matched string with `g`             |
| `s.matchAll(re)`                    | An iterator of match arrays, requires `g`                 |
| `s.replace(re, rep)`                | Replaces the first match, or every match with `g`         |
| `s.replaceAll(re, rep)`             | Replaces every match, requires `g`                        |
| `s.split(re, limit?)`               | Splits on matches; capture groups are included            |
| `s.search(re)`                      | Index of the first match, or `-1`                         |

Replacement strings understand `$1` to `$99`, `$<name>`, `$&` (the match), `` $` `` (text before),
`$'` (text after) and `$$`. A replacer function receives the match, every group, the offset, the
whole string and, when the pattern has named groups, the groups object.

## Modules

Every `.jsq` file is an ES module. Imports are resolved and the imported modules are evaluated
before the importing module's body runs. Each module is evaluated once per process and shared by
every importer.

### Exports

```js
export const VERSION = "1.0.0"
export let hits = 0
export function track() {
  hits++
}
export class Tracker {}

const secret = 42
export { secret as answer }

export default function main() {
  return "default export"
}
```

`export default` accepts any expression, function or class.

### Imports

```js
import main from "./lib"
import { VERSION, answer as theAnswer } from "./lib"
import * as lib from "./lib.jsq"
import { readFile, writeFile } from "squirt:fs"
import pad from "left-pad"
```

| Form                                   | Binds                                                |
| -------------------------------------- | ---------------------------------------------------- |
| `import x from "m"`                    | The default export of `m` as `x`                     |
| `import { a, b } from "m"`             | The named exports `a` and `b`                        |
| `import { a as c } from "m"`           | The named export `a` as `c`                          |
| `import * as ns from "m"`              | An object holding every export of `m`; `default` included |

### Resolution

A specifier is resolved in this order:

1. **Builtin modules**: specifiers starting with `squirt:` refer to builtin modules. The only
   builtin module in 0.1.0 is `squirt:fs`.
2. **Relative paths**: specifiers starting with `./` or `../` are resolved against the directory
   of the importing file. For `./x`, the loader tries:
   1. `x` as written, if it is a file
   2. `x.jsq`
   3. `x/index.jsq`
3. **Bare names**: any other specifier, such as `left-pad`, is a package installed in
   `nude_modules/<name>/`. The entry file is the `main` field of that package's `squirt.json`,
   or `index.jsq` when there is no manifest or no `main`.

A specifier that cannot be resolved is an error.

### `squirt:fs`

Synchronous file system access. Paths are relative to the current working directory. Failures
throw an `Error` with the operating system's message.

| Function                    | Description                                       |
| --------------------------- | ------------------------------------------------- |
| `readFile(path)`            | Returns the file contents as a string             |
| `writeFile(path, data)`     | Creates or truncates the file and writes `data`   |
| `appendFile(path, data)`    | Appends `data`, creating the file if needed       |
| `exists(path)`              | Returns `true` if the path exists                 |
| `remove(path)`              | Deletes the file                                  |

```js
import { writeFile, appendFile, readFile, exists, remove } from "squirt:fs"

writeFile("notes.txt", "one\n")
appendFile("notes.txt", "two\n")
console.log(readFile("notes.txt").split("\n").length, exists("notes.txt"))
remove("notes.txt")
console.log(exists("notes.txt"))
```

```text
3 true
false
```

## Builtins

### Global functions and values

| Name                       | Description                                                  |
| -------------------------- | ------------------------------------------------------------ |
| `parseInt(text, radix?)`   | Parses a leading integer, `NaN` if none                      |
| `parseFloat(text)`         | Parses a leading decimal number, `NaN` if none               |
| `isNaN(value)`             | `true` if the value converts to `NaN`                        |
| `isFinite(value)`          | `true` if the value converts to a finite number              |
| `NaN`, `Infinity`, `undefined` | The usual global values                                  |

### `console`

| Method                 | Stream |
| ---------------------- | ------ |
| `console.log(...args)`   | stdout |
| `console.info(...args)`  | stdout |
| `console.warn(...args)`  | stderr |
| `console.error(...args)` | stderr |

Arguments are separated by a single space. Strings print as is; arrays, objects, `Map` and `Set`
print in a readable inspected form.

### Global objects

| Object   | Highlights                                                                                     |
| -------- | ---------------------------------------------------------------------------------------------- |
| `Math`   | Constants such as `PI` and `E`; `abs`, `floor`, `ceil`, `round`, `trunc`, `sign`, `min`, `max`, `pow`, `sqrt`, `random` and friends |
| `Number` | Conversion with `Number(value)`, number checks and constants, `toFixed` and `toString` on numbers |
| `String` | Conversion with `String(value)`, `length`, indexing, `slice`, `split`, `trim`, `toUpperCase`, `toLowerCase`, `includes`, `indexOf`, `startsWith`, `endsWith`, `replace`, `repeat` and friends |
| `Array`  | `Array.isArray`, `length`, `push`, `pop`, `shift`, `unshift`, `slice`, `splice`, `concat`, `join`, `indexOf`, `includes`, `find`, `findIndex`, `map`, `filter`, `reduce`, `forEach`, `some`, `every`, `sort`, `reverse` and friends |
| `Object` | `Object.keys`, `Object.values`, `Object.entries`, `Object.assign` and friends                   |
| `JSON`   | `JSON.stringify(value)` and `JSON.parse(text)`                                                 |
| `Map`    | `new Map(entries?)`, `set`, `get`, `has`, `delete`, `clear`, `size`, `forEach`                 |
| `Set`    | `new Set(values?)`, `add`, `has`, `delete`, `clear`, `size`, `forEach`                         |
| Errors   | `Error`, `TypeError`, `RangeError`, `ReferenceError`, `SyntaxError`                            |
| Weak     | `WeakMap` (`get`, `set`, `has`, `delete`), `WeakSet` (`add`, `has`, `delete`), `WeakRef` (`deref`) with object keys |
| Timing   | `performance.now()` for a monotonic clock in milliseconds, `Date.now()` for epoch milliseconds |
| `squirt` | `squirt(n = 3)` returns `n` droplets, the one builtin no other language ships                   |

Builtin methods follow their ECMAScript definitions. The exact set implemented by a given
version is defined in `src/builtins/`; calling a method that does not exist throws a `TypeError`
because the property is `undefined`.

```js
const words = ["vm", "jsq", "squirt", "jsq"]
const unique = new Set(words)
const lengths = new Map()
for (const w of unique) {
  lengths.set(w, w.length)
}

console.log(unique.size, lengths.get("squirt"))
console.log(JSON.stringify({ words: words.slice(0, 2), max: Math.max(1, 9, 4) }))
console.log(Object.keys({ a: 1, b: 2 }).join("|"), parseInt("42px"), Number("3.5") * 2)
```

```text
3 6
{"words":["vm","jsq"],"max":9}
a|b 42 7
```

### `process`

| Member                | Description                                                |
| --------------------- | ---------------------------------------------------------- |
| `process.argv`        | Array of strings with the program's command-line arguments |
| `process.env`         | Object with the environment variables                      |
| `process.exit(code?)` | Exits immediately with `code`, `0` by default              |

```js
const name = process.env.USER ?? "anonymous"
console.log(`hello ${name}, ${process.argv.length} args`)
if (process.argv.includes("--fail")) {
  process.exit(1)
}
```

## Memory management

Values are reference counted, so most garbage is freed the moment it becomes unreachable. Cycles,
such as an object that stores a closure capturing that same object, a class and its prototype, or
two objects pointing at each other, are reclaimed by a cycle collector that runs automatically
as the program allocates. It needs no annotations and never frees anything still reachable from
running code.

The `JSQ_GC_THRESHOLD` environment variable sets the minimum number of allocations between
collections (default 200000). Small values are useful for stress testing.

## CLI reference

### `jsq`

| Invocation                    | Behavior                                                         |
| ----------------------------- | ---------------------------------------------------------------- |
| `jsq`                         | Starts the REPL. Each input is evaluated and its value printed.  |
| `jsq <file.jsq> [args...]`    | Runs the file as the entry module. `args` are exposed through `process.argv`. |
| `jsq -e "<code>"`             | Evaluates the code string as a module.                           |
| `jsq --disasm <file.jsq>`     | Compiles the file and prints its bytecode without running it.    |
| `jsq --version`               | Prints the version.                                              |

Exit status is `0` on success, the value passed to `process.exit`, or non-zero after a syntax
error or an uncaught exception.

### `vrotdaem`

| Command                        | Behavior                                                                  |
| ------------------------------ | ------------------------------------------------------------------------- |
| `vrotdaem init [--name <n>]`   | Creates `squirt.json` in the current directory.                           |
| `vrotdaem add <name> <spec>`   | Adds or replaces a dependency in `squirt.json`.                           |
| `vrotdaem remove <name>`       | Removes a dependency from `squirt.json`.                                  |
| `vrotdaem gluboko`             | Installs every dependency into `nude_modules/<name>`.                     |
| `vrotdaem gluboko --force`     | Deletes `nude_modules/` and reinstalls everything from scratch.           |
| `vrotdaem run [args...]`       | Runs the manifest's `main` with `jsq` semantics, passing `args` along.    |

Dependency specs:

| Spec               | Meaning                                                                 |
| ------------------ | ----------------------------------------------------------------------- |
| `file:<path>`      | A local directory, relative to `squirt.json` or absolute                |
| `git:<url>`        | A git repository at its default branch; requires `git` on `PATH`        |
| `git:<url>#<ref>`  | A git repository at a branch, tag or commit                             |

### `squirt.json`

```json
{
  "name": "my-app",
  "version": "0.1.0",
  "main": "main.jsq",
  "dependencies": {
    "utils": "file:../utils",
    "left-pad": "git:https://github.com/someone/left-pad#v1.0.0"
  }
}
```

| Field          | Description                                                     |
| -------------- | --------------------------------------------------------------- |
| `name`         | Package name, also the bare specifier other packages import     |
| `version`      | Package version, SemVer recommended                             |
| `main`         | Entry file, used by `vrotdaem run` and by bare-name imports     |
| `dependencies` | Map from package name to dependency spec                        |

## Not yet supported

These are on the roadmap and currently produce a `SyntaxError` or are absent:

- `FinalizationRegistry`
- `Intl` and locale-aware formatting beyond `en-US`
- Getters on the global object (`globalThis`)
- Tracing garbage collection
- JIT compilation
