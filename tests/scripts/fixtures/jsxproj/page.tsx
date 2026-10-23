interface ItemProps {
  label: string
  done?: boolean
}
const Item = ({ label, done }: ItemProps) => <li class={done ? "done" : "todo"}>{label}{done && " ✓"}</li>
const pick = <T,>(items: T[]): T => items[0]
export const page = (items: ItemProps[]) => (
  <>
    <h1 title="a &amp; b">Tasks &mdash; {items.length}</h1>
    <ul data-count={String(items.length)}>
      {items.map((it) => <Item key={it.label} {...it} />)}
    </ul>
    {/* nothing here */}
    <p>
      first line
      second   line
    </p>
    <input disabled />
    <span>{pick(["x", "y"])}</span>
  </>
)
