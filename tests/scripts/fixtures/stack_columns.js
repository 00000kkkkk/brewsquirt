const first = new Error("first").stack.split("\n")[1].trim().replace(/[^ (]*[\\/]/, "")
const frame = (e, i = 1) => e.stack.split("\n")[i].trim().replace(/[^ (]*[\\/]/, "")
function cap() { return frame(new Error("x"), 2) }
function Cap() { this.s = cap() }
const o = { cap, n: { cap }, Cap }
console.log(first)
console.log(cap())
console.log(o.cap())
console.log(o.n.cap())
console.log(o["cap"]())
console.log((0, cap)())
console.log(o?.cap())
console.log(o
  .cap())
console.log(new Cap().s)
console.log(cap`tag`)
const fail = (f) => {
  try {
    f()
  } catch (e) {
    return `${e.constructor.name} ${frame(e)}`
  }
}
const u = undefined
const s = Symbol("s")
const q = {}
console.log(fail(() => null.x))
console.log(fail(() => u.foo.bar))
console.log(fail(() => { u.x = 1 }))
console.log(fail(() => { u["x"] = 1 }))
console.log(fail(() => q.nope()))
console.log(fail(() => { missing() }))
console.log(fail(() => 1n + 1))
console.log(fail(() => `${s}`))
console.log(fail(() => { for (const x of u) {} }))
console.log(fail(() => [...u]))
console.log(fail(() => u[s]))
console.log(fail(() => new u()))
console.log(fail(() => { throw new RangeError("r") }))
const saved = Error.prepareStackTrace
Error.prepareStackTrace = (err, sites) => sites.map((c) => `${c.getLineNumber()}:${c.getColumnNumber()}`).join(" ")
const sites = new Error("sites").stack
Error.prepareStackTrace = saved
console.log(sites.split(" ")[0])
