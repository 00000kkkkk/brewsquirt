import path = require("node:path")
const join = (...parts: string[]): string => path.posix.join(...parts)
export = { join, kind: "cts" as const }
