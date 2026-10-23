const vm = require("vm")
const head = "'use strict';async ()=>{{"
const fn = vm.runInThisContext(head + "return new Error('a').stack.split('\\n')[1].trim()\n}}", { filename: "/v/a.js", lineOffset: 0, columnOffset: -head.length })
fn().then(console.log)
console.log(new vm.Script("\nnew Error('b').stack.split('\\n')[1].trim()", { filename: "/v/b.js", lineOffset: 10, columnOffset: 5 }).runInThisContext())
console.log(new vm.Script("new Error('b2').stack.split('\\n')[1].trim()", { filename: "/v/b2.js", lineOffset: 10, columnOffset: 5 }).runInThisContext())
console.log(vm.compileFunction("return new Error('c').stack.split('\\n')[1].trim()", [], { filename: "/v/c.js" })())
console.log(vm.compileFunction("\n  return new Error('d').stack.split('\\n')[1].trim()", ["x"], { filename: "/v/d.js", lineOffset: 3 })())
try { vm.runInThisContext("let = ;", { filename: "/v/e.js", columnOffset: 4 }) } catch (e) { console.log(e.name) }
