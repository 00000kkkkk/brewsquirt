const addon = require("./addon.node")
console.log(Object.keys(addon).sort().join(","))
console.log(addon.napiVersion >= 8)
console.log(addon.hello("world"), addon.hello("мир ✓"))
console.log(addon.add(2, 3.5))
console.log(addon.makeObject())
console.log(addon.typeNames(undefined, "s", 1, {}))
console.log(addon.bufferSum(Buffer.from([1, 2, 3, 250])))
console.log(addon.reverseBuffer(Buffer.from("abc")).toString())
const obj = { a: 1 }
console.log(addon.keep(obj) === obj)
console.log(addon.callBack(x => ({ value: x * 2 }), 21))
try {
  addon.callBack(() => { throw new Error("from js") }, 0)
} catch (e) {
  console.log("rethrown", e.message)
}
try {
  addon.fail()
} catch (e) {
  console.log(e instanceof Error, e.code, e.message)
}
const c = new addon.Counter(10)
c.inc()
c.inc(5)
console.log(c.value, c instanceof addon.Counter, addon.Counter.name)
for (let i = 0; i < 1000; i++) new addon.Counter(i).inc()
addon.asyncSum(100000).then(v => console.log("async", v))
const seen = []
addon.ticks(v => {
  seen.push(v)
  if (seen.length === 5) console.log("ticks", seen.join(" "))
}, 5)
process.on("exit", () => console.log("exit", seen.length))
