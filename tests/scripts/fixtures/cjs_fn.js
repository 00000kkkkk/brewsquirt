function make(n) {
  return n * 2
}
make.helper = (s) => `helper:${s}`
make.VERSION = "1.0"
module.exports = make
