const store = new Map<object, Map<string | symbol | undefined, Map<string, unknown>>>()
const slot = (target: object, key: string | symbol | undefined) => {
  if (!store.has(target)) store.set(target, new Map())
  const byKey = store.get(target)!
  if (!byKey.has(key)) byKey.set(key, new Map())
  return byKey.get(key)!
}
Object.assign(Reflect, {
  metadata: (k: string, v: unknown) => (target: object, key?: string | symbol) => {
    slot(target, key).set(k, v)
  },
  getMetadata: (k: string, target: object, key?: string | symbol) => slot(target, key).get(k),
})

export const events: string[] = []

function Entity(table: string): ClassDecorator {
  return (target) => {
    events.push(`entity ${target.name} -> ${table}`)
  }
}

function Column(target: object, key: string) {
  const type = (Reflect as any).getMetadata("design:type", target, key)
  events.push(`column ${key}: ${type?.name ?? type}`)
}

function Validate(target: object, key: string, desc: PropertyDescriptor) {
  const original = desc.value
  desc.value = function (this: unknown, ...args: unknown[]) {
    if (args.some((a) => a === undefined)) throw new TypeError(`${key} needs every argument`)
    return original.apply(this, args)
  }
}

function Tag(name: string) {
  return (target: object, key: string | symbol | undefined, index: number) => {
    events.push(`param ${String(key ?? "constructor")}#${index} ${name}`)
  }
}

export class Address {
  constructor(public city: string) {}
}

@Entity("users")
export class User {
  @Column id: number
  @Column name: string
  @Column address: Address
  @Column roles: string[]
  @Column active: boolean | undefined

  constructor(@Tag("db") private readonly source: string, id: number) {
    this.id = id
    this.name = "anon"
  }

  @Validate
  rename(@Tag("value") name: string, reason?: string): User {
    this.name = name
    return this
  }

  static types() {
    const get = (Reflect as any).getMetadata
    return [
      get("design:paramtypes", User).map((t: Function) => t.name).join(","),
      get("design:paramtypes", User.prototype, "rename").map((t: Function) => t.name).join(","),
      get("design:returntype", User.prototype, "rename").name,
    ]
  }
}
