process.on("unhandledRejection", () => console.log("listener"))
await Promise.reject(new TypeError("entry failed"))
