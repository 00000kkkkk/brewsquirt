const v8 = require("v8")
const { fork } = require("child_process")
if (process.argv[2] === "child") {
  process.on("message", (m) => {
    m.back = true
    m.me = m
    process.send(m)
    process.disconnect()
  })
} else {
  const a = { x: 1, m: new Map([[1, { n: 2n }]]), s: new Set(["a"]), d: new Date(0), r: /x/g, u8: new Uint8Array([1, 2]), e: new TypeError("bad"), arr: [1, , 3] }
  a.self = a
  const b = v8.deserialize(v8.serialize(a))
  console.log(b.self === b, b.m.get(1).n, [...b.s], b.d.toISOString(), b.r.flags, b.u8, b.e instanceof TypeError, b.e.message, 1 in b.arr, Buffer.isBuffer(v8.serialize(1)))
  const c = fork(__filename, ["child"], { serialization: "advanced" })
  const msg = { big: 10n, when: new Date(5), map: new Map([["k", 1]]) }
  c.on("message", (m) => console.log("parent got", m.back, m.big, m.when.getTime(), m.map.get("k"), m.me === m))
  c.on("exit", (code) => console.log("child exit", code))
  c.send(msg)
}
