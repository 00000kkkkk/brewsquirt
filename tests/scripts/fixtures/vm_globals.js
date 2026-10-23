const vm = require("vm")
const ctx = vm.createContext({ a: 1 })
vm.runInContext("var x = 10; function f() { return x + a } let y = 2; const z = 3; b = 4; this.c = 5", ctx)
console.log(Object.keys(ctx).sort(), ctx.x, typeof ctx.f, ctx.y, ctx.b, ctx.c, typeof globalThis.b)
console.log(vm.runInContext("[x, f(), y, z, b, c, typeof y, typeof missing]", ctx))
vm.runInContext("x = 11; y = 12", ctx)
console.log(ctx.x, vm.runInContext("y", ctx))
for (const code of ["z = 4", "let y = 1", "missing + 1", "'use strict'; undeclared = 1", "early; let early = 1"]) {
  try {
    vm.runInContext(code, ctx)
  } catch (e) {
    console.log(e.name, e.message)
  }
}
console.log(vm.runInContext("delete c", ctx), "c" in ctx, vm.runInContext("delete y", ctx))
console.log(vm.runInContext("(function () { return this })() === globalThis && this === globalThis", ctx))
const ev = vm.runInContext("eval", ctx)
console.log(vm.runInContext("eval === globalThis.eval", ctx), ev === eval, ev("x + 1"), ev(7))
console.log(vm.runInContext("(function () { var local = 5; return eval('local + x') })()", ctx))
console.log(vm.runInContext("typeof Array === 'function' && [1, 2].map((n) => n * a).join()", ctx))
class Win {
  get document() {
    return { title: "doc" }
  }
}
const win = new Win()
vm.createContext(win)
console.log(vm.runInContext("document.title + ' ' + typeof document", win))
const fn = vm.compileFunction("return a + extra", ["extra"], { parsingContext: ctx })
console.log(fn(100))
console.log(vm.runInNewContext("var n = typeof require; n", { }), vm.runInNewContext("q * 2", { q: 21 }))
