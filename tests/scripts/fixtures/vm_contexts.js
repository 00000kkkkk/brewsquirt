const vm = require("vm")
const ctx = vm.createContext()
const g = vm.runInContext("this", ctx)
console.log(typeof g)
g.test = () => "ok"
g.global = g
const fn = new vm.Script("({'Object.<anonymous>': function () { return test() + typeof global + typeof globalThis.test }})", { filename: "t.js" }).runInContext(ctx)
console.log(fn["Object.<anonymous>"]())
const ctx2 = vm.createContext({ answer: 42 })
console.log(vm.runInContext("answer + 1", ctx2), vm.runInContext("typeof answer", ctx2))
console.log(vm.runInThisContext("1 + 2"), new vm.Script("let scoped = 5; scoped * 2", { filename: "my.js" }).runInThisContext())
try {
  vm.runInThisContext("\n\nnull.x", { filename: "virtual.js" })
} catch (e) {
  console.log(e instanceof TypeError, e.stack.includes("virtual.js:3"))
}
const wrapped = vm.runInThisContext("(function (a) { return a + this.b })", { filename: "wrap.js" })
console.log(wrapped.call({ b: 2 }, 1))
const sandbox = vm.createContext({ base: 10 })
const compiled = vm.compileFunction("return base + x", ["x"], { parsingContext: sandbox, filename: "cf.js" })
console.log(compiled(5), typeof compiled)
const plain = vm.compileFunction("return a * b", ["a", "b"])
console.log(plain(6, 7))
