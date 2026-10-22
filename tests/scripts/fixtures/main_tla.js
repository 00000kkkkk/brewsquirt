const value = await Promise.resolve(7)
console.log("top-level await in a .js entry", value)
