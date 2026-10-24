const net = require("net")
const http = require("http")
const fs = require("fs")
const path = require("path").join(require("os").tmpdir(), "jsq_ipc_" + process.pid + ".sock")
const server = net.createServer((sock) => {
  console.log("server conn", sock.remoteAddress, sock.remoteFamily)
  let buf = ""
  sock.on("data", (d) => {
    buf += d
    if (buf.includes("\r\n\r\n")) {
      sock.end("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello")
    } else if (buf === "ping") {
      sock.end("pong")
    }
  })
})
server.listen(path, () => {
  console.log("listening", server.address(), fs.existsSync(path))
  const c = net.connect(path, () => {
    console.log("client connected", c.remoteAddress, c.remotePort)
    c.write("ping")
  })
  c.on("data", (d) => console.log("client got", String(d)))
  c.on("close", () => {
    http.get({ socketPath: path, path: "/v1/info" }, (res) => {
      let body = ""
      res.on("data", (d) => (body += d))
      res.on("end", () => {
        console.log("http", res.statusCode, res.headers["content-type"], body)
        const bad = net.connect({ path: "/tmp/does-not-exist.sock" })
        bad.on("error", (e) => {
          console.log("error", e.code, e.syscall, e.address)
          server.close(() => console.log("closed", fs.existsSync(path)))
        })
      })
    })
  })
})
