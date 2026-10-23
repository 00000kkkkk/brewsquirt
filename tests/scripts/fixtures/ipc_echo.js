process.on("message", (m) => {
  process.send({ echo: m, refCounted: typeof process.channel.refCounted })
  process.disconnect()
})
