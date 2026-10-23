const named = (n) => (cls, ctx) => { cls.tagName = n + ":" + ctx.name; return cls }
export default @named("d") class {}
export @named("a") class A {}
@named("b") export class B {}
export const C = @named("c") class C2 {}
