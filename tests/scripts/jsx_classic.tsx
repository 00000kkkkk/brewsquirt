/** @jsx h */
/** @jsxFrag Group */
type Node = { tag: string; props: Record<string, unknown> | null; children: unknown[] }
const h = (tag: unknown, props: Record<string, unknown> | null, ...children: unknown[]): Node => ({
  tag: typeof tag === "function" ? tag.name : String(tag),
  props,
  children,
})
const Group = "group"
function Badge({ text }: { text: string }) {
  return <b>{text}</b>
}
const nested = <p label={<i>inner</i>} />
const tree = (
  <Group>
    <Badge text="new" />
    <a href={`/item/${1 + 1}`}>link</a>
    {[1, 2].map((n) => <em key={n}>{n}</em>)}
  </Group>
)
console.log(JSON.stringify(tree))
console.log(JSON.stringify(nested))
