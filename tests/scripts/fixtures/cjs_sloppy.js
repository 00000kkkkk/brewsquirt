implicitGlobal = 0o10 + 010
var await = 1, yield = 2, let = 3, static = 4
function plainThis() {
  return this === globalThis
}
function noThis(a) {
  return typeof a
}
var scope = { inner: "from with" }
var seen
with (scope) {
  seen = inner
}
module.exports = {
  implicit: implicitGlobal,
  words: [await, yield, let, static],
  plainThis: plainThis(),
  primitiveThis: function () {
    return typeof this
  }.call(5),
  noThis: noThis.call(undefined, 1),
  seen,
}
