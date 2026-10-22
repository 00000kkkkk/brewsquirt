console.log("main runs once", typeof module, typeof exports, typeof __dirname, typeof __filename)
const id = module.id === "." ? "." : require("node:path").basename(module.id)
console.log(require.main === module, id, process.mainModule === module, this === module.exports)
module.exports = { value: 42 }
if (require.main === module) console.log("entry point")
