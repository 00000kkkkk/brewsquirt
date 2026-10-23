"use strict"
const top = () => new Error().stack.split("\n")[2].trim().replace(/ \(.*/, "")
class K { m() { return top() } static s() { return top() } get g() { return top() } }
class Sub extends K { m() { return top() } }
function plain() { return top() }
function F() {}
F.s = function () { return top() }
F.t = function tt() { return top() }
const o = { m() { return top() }, f: function () { return top() }, arrow: () => top() }
o.alias = plain
const k = new K()
const bound = plain.bind(o)
const lines = [
  k.m(),
  K.s(),
  k.g,
  new Sub().m(),
  plain(),
  o.m(),
  o.f(),
  o.arrow(),
  o.alias(),
  bound(),
  plain.call(k),
  (function named() { return top() }).call(5),
  F.s(),
  F.t(),
  new (class Q { constructor() { this.v = top() } })().v,
  [1].map(function () { return top() }, o)[0],
]
for (const line of lines) console.log(line)
const saved = Error.prepareStackTrace
Error.prepareStackTrace = (err, sites) => {
  const s = sites[0]
  return [s.getTypeName(), s.getFunctionName(), s.getMethodName(), s.isToplevel(), s.isConstructor()].join(" ")
}
console.log(new Error().stack)
console.log((() => new Error().stack)())
console.log({ go() { return new Error().stack } }.go())
Error.prepareStackTrace = saved
