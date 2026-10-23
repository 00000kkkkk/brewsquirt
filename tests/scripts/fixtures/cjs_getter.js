let cached
Object.defineProperty(module, "exports", {
  get: () => cached ?? (cached = { lazy: 42, at: "getter" }),
})
