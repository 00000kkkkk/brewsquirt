function log<This, Args extends any[], Ret>(target: (this: This, ...args: Args) => Ret, context: ClassMethodDecoratorContext<This, (this: This, ...args: Args) => Ret>) {
  return function (this: This, ...args: Args): Ret {
    console.log(`calling ${String(context.name)}`)
    return target.call(this, ...args)
  }
}
function bound(_t: unknown, context: ClassMethodDecoratorContext) {
  context.addInitializer(function (this: any) {
    this[context.name] = this[context.name].bind(this)
  })
}
class Greeter {
  constructor(private readonly who: string) {}
  @log @bound
  greet(): string {
    return "hello " + this.who
  }
  @log static create(): Greeter {
    return new Greeter("static")
  }
}
const g = new Greeter("ts")
const f = g.greet
console.log(f(), Greeter.create().greet())
