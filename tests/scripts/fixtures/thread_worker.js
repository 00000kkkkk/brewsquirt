const { parentPort, workerData, isMainThread, threadId } = require("worker_threads")
const counter = new Int32Array(workerData.shared)
for (let i = 0; i < 1000; i++) Atomics.add(counter, 0, 1)
parentPort.postMessage({
  isMainThread,
  threadId,
  date: workerData.when.toISOString(),
  map: [...workerData.map],
  bytes: [...new Uint8Array(workerData.bytes)],
})
parentPort.once("message", ({ port }) => {
  port.postMessage({ hello: "through a transferred port" })
  Atomics.store(counter, 1, 1)
  Atomics.notify(counter, 1)
  const woke = Atomics.wait(counter, 2, 0, 10000)
  port.postMessage({ woke, value: Atomics.load(counter, 2) })
  port.close()
})
