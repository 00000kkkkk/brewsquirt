import { User, events } from "./fixtures/decproj/model.ts"
const user = new User("pg", 1).rename("ann")
console.log(user.name, user.id)
try {
  user.rename(undefined as unknown as string)
} catch (e) {
  console.log((e as Error).message)
}
console.log(events.join("\n"))
console.log(User.types().join(" | "))
