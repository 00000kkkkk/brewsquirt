import type { Buffer as NodeBuffer } from "node:buffer"
import { type Stats, readFileSync } from "node:fs"
import * as path from "node:path"

interface Point {
  readonly x: number
  y?: number
  [extra: string]: unknown
}

type Pair<A, B = A> = [first: A, second: B]
type Shape = { kind: "circle"; r: number } | { kind: "square"; side: number }
type Unwrap<T> = T extends Promise<infer U extends object> ? U : T extends Array<infer E> ? E : never
type Getters<T> = { [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K] }
type Fn = <T>(value: T) => T
type Ctor = abstract new (...args: any[]) => object

declare const injected: string
declare function external(x: number): void
declare global {
  interface Window {
    custom: string
  }
}
declare module "virtual" {
  export const v: number
}

enum Direction {
  Up = 1,
  Down,
  Left = Up + 10,
  Right,
}

const enum Flags {
  None = 0,
  Read = 1 << 0,
  Write = 1 << 1,
  All = Read | Write,
}

enum Label {
  Ok = "ok",
  Fail = `fail`,
}

namespace Geometry {
  export const origin: Point = { x: 0, y: 0 }
  export function distance(a: Point, b: Point): number {
    return Math.hypot(a.x - b.x, (a.y ?? 0) - (b.y ?? 0))
  }
  export namespace Inner {
    export const unit = 1
  }
  const hidden = "secret"
  export const reveal = () => hidden.length
}

namespace Deep.Nested.Space {
  export const value = "deep"
}

function area(shape: Shape): number {
  switch (shape.kind) {
    case "circle":
      return Math.round(Math.PI * shape.r ** 2)
    case "square":
      return shape.side * shape.side
  }
}

function overloaded(x: string): string
function overloaded(x: number): number
function overloaded(x: string | number): string | number {
  return typeof x === "string" ? x.toUpperCase() : x * 2
}

function identity<const T extends readonly unknown[]>(value: T): T {
  return value
}

function assertString(value: unknown): asserts value is string {
  if (typeof value !== "string") throw new TypeError("not a string")
}

function isNumber(value: unknown): value is number {
  return typeof value === "number"
}

function withThis(this: { base: number }, add: number): number {
  return this.base + add
}

abstract class Animal<TName extends string = string> implements Iterable<string> {
  static count: number = 0
  protected abstract readonly sound: string
  declare readonly tag: string
  #secret: number = 42
  private readonly born = Date.now() > 0
  abstract describe(): string

  constructor(public readonly name: TName, protected legs: number = 4) {
    Animal.count++
  }

  speak(): string {
    return `${this.name} says ${this.sound}`
  }

  get secret(): number {
    return this.#secret
  }

  *[Symbol.iterator](): Iterator<string> {
    yield this.name
    yield String(this.legs)
  }
}

class Dog extends Animal<"dog"> {
  protected readonly sound = "woof"
  public override describe(): string {
    return `${this.speak()} on ${this.legs} legs`
  }
  constructor(private owner?: string) {
    super("dog")
  }
  ownerName(): string {
    return this.owner ?? "nobody"
  }
}

class Box<T> {
  constructor(private value: T) {}
  map<U>(fn: (value: T) => U): Box<U> {
    return new Box<U>(fn(this.value))
  }
  get(): T {
    return this.value
  }
}

const generic = <T,>(items: T[]): T | undefined => items[0]
const asyncGeneric = async <T>(value: T): Promise<T> => value
const typedArrow = (a: number, b?: number): number => a + (b ?? 0)
const returnsObject = (): { ok: boolean } => ({ ok: true })
const predicateArrow = (v: unknown): v is string => typeof v === "string"

let definite!: number
definite = 5
const maybe: string | null = Math.random() > 2 ? null : "present"
const nonNull = maybe!.length
const asserted = <number>(<unknown>"7".length)
const castChain = ("x" as unknown as string).repeat(2)
const satisfied = { a: 1, b: [1, 2] } satisfies Record<string, unknown>
const constTuple = [1, "two"] as const
const big: Map<string, Array<Map<string, number>>> = new Map<string, Array<Map<string, number>>>()
big.set("k", [new Map([["n", 1]])])
const shifted = (8 >> 1) + (16 >>> 2)
const comparison = 1 < 2 && 3 > 2
const instantiated = Array<number>
const point: Point = { x: 1, y: 2, label: "p" }

console.log(Direction.Up, Direction.Down, Direction.Left, Direction.Right, Direction[2])
console.log(Flags.All, Flags.Read | Flags.Write, Label.Ok, Label.Fail, Object.keys(Label))
console.log(Geometry.distance(Geometry.origin, { x: 3, y: 4 }), Geometry.Inner.unit, Geometry.reveal(), Deep.Nested.Space.value)
console.log(area({ kind: "circle", r: 1 }), area({ kind: "square", side: 3 }))
console.log(overloaded("hi"), overloaded(21), identity([1, 2] as const).length)
assertString("fine")
console.log(isNumber(3), isNumber("3"), withThis.call({ base: 10 }, 5))
const dog = new Dog("ann")
console.log(dog.describe(), dog.name, dog.ownerName(), dog.secret, [...dog].join("/"), Animal.count, "tag" in dog)
console.log(new Box(2).map((n) => `${n * 3}`).get(), generic(["a", "b"]), await asyncGeneric(9))
console.log(typedArrow(1), typedArrow(1, 2), returnsObject().ok, predicateArrow("s"))
console.log(definite, nonNull, asserted, castChain, satisfied.b.length, constTuple[1], big.get("k")![0].get("n"))
console.log(shifted, comparison, instantiated === Array, point.label, typeof injected === "undefined" || injected)
console.log(path.basename("/a/b.ts"), typeof readFileSync, typeof (null as unknown as Stats))

type Handler = (event: { type: string }) => void
const handlers: Record<string, Handler> = {}
handlers.click = (e) => console.log("handled", e.type)
handlers.click({ type: "click" })

const obj = {
  method<T>(x: T): T {
    return x
  },
  get value(): number {
    return 1
  },
}
console.log(obj.method<string>("m"), obj.value)

for (let i: number = 0, j: number = 2; i < j; i++) console.log("loop", i)
try {
  throw new Error("caught")
} catch (err: unknown) {
  console.log((err as Error).message)
}
