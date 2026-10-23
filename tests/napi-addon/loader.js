const m = { exports: {} }
process.dlopen(m, require("path").join(__dirname, "addon.node"))
console.log("dlopen", m.exports.add(2, 3), m.exports.hello("dl"))
