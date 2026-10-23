type Props = Record<string, unknown> & { children?: unknown }
export const Fragment = Symbol("Fragment")
const render = (node: unknown): string => {
  if (Array.isArray(node)) return node.map(render).join("")
  if (node === null || node === undefined || node === false || node === true) return ""
  return String(node)
}
export function jsx(type: unknown, props: Props, key?: unknown): string {
  if (typeof type === "function") return type(props)
  const { children, ...rest } = props
  const inner = render(children)
  if (type === Fragment) return inner
  const attrs = Object.entries(rest).map(([k, v]) => (v === true ? ` ${k}` : ` ${k}="${v}"`)).join("")
  return `<${String(type)}${key === undefined ? "" : ` key="${key}"`}${attrs}>${inner}</${String(type)}>`
}
export const jsxs = jsx
