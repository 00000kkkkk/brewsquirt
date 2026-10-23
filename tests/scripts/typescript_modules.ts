import { describe, defaults, Options, type Mode } from "./fixtures/ts_lib.js"
import { createRequire } from "node:module"
const require = createRequire(import.meta.url)
const common: { join: (...parts: string[]) => string; kind: string } = require("./fixtures/ts_common.cts")
const options: Options = { ...defaults, verbose: true }
const mode: Mode = "fast"
console.log(describe(mode), describe("slow", options))
console.log(common.kind, common.join("a", "b"))
const lib = await import("./fixtures/ts_lib.ts")
console.log(Object.keys(lib).sort().join(","))
