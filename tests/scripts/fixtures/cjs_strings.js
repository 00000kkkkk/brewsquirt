var banner = "export default from a string"
var note = `import { nothing } from "nowhere"`
var lib = {}
lib.describe = function () {
  return banner.length + note.length
}
if (typeof exports !== "undefined") {
  exports.describe = lib.describe
  exports.kind = "commonjs"
}
