const M = module.constructor
module.exports = [M === require("module"), typeof M.createRequire, typeof M.isBuiltin, module instanceof M, Object.keys(module).join(",")]
