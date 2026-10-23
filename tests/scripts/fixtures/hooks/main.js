const Module = require("module")
console.log(typeof Module, typeof Module._extensions[".js"], typeof Module._load, Module.wrap("x").length, module instanceof Module, typeof Module.createRequire, Module.builtinModules.includes("fs"))
const orig = Module._extensions[".js"]
Module._extensions[".js"] = function (mod, filename) {
  if (filename.endsWith("a.js")) {
    const code = require("fs").readFileSync(filename, "utf8").replace("PLAIN", "HOOKED")
    return mod._compile(code, filename)
  }
  return orig(mod, filename)
}
Module._extensions[".custom"] = function (mod, filename) { mod._compile(require("fs").readFileSync(filename, "utf8"), filename) }
console.log(require("./a"), require("./b.custom"))
console.log(Module._resolveFilename("./a", module).endsWith("a.js"), Module._nodeModulePaths(__dirname)[0].endsWith("node_modules"))
const m = new Module("/x/y.js")
m._compile("module.exports = 7 * 6", "/x/y.js")
console.log(m.exports)
