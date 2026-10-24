use brewsquirt::Vm;
use brewsquirt::compiler::Mode;
use brewsquirt::gc;

fn quiet_vm() -> Vm {
    Vm::with_output(
        Vec::new(),
        Box::new(std::io::sink()),
        Box::new(std::io::sink()),
    )
}

fn run(vm: &mut Vm, src: &str) {
    if let Err(e) = vm.eval("<gc-test>", src, Mode::Module) {
        panic!("{}", vm.format_uncaught(&e));
    }
}

const CYCLES: &str = "for (let i = 0; i < 3000; i++) {
  const a = { i }
  const b = { a }
  a.b = b
  a.self = a
  class Node {
    next() {
      return Node
    }
  }
  const n = new Node()
  n.me = n
  const f = () => f
  const m = new Map([[a, b]])
  b.m = m
}";

const LIVE: &str = "const keep = []
for (let i = 0; i < 500; i++) {
  const o = { i }
  o.self = o
  keep.push(o)
}
const counter = (() => {
  let n = 0
  return () => ++n
})()
for (let i = 0; i < 300000; i++) counter()
if (keep.length !== 500 || keep[499].self.i !== 499) throw new Error('lost live data')
if (counter() !== 300001) throw new Error('lost closure state')";

#[test]
fn reclaims_unreachable_cycles() {
    let mut vm = quiet_vm();
    gc::collect();
    let baseline = gc::live_objects();
    gc::set_enabled(false);
    run(&mut vm, CYCLES);
    gc::set_enabled(true);
    let before = gc::live_objects();
    assert!(
        before > baseline + 3000,
        "cycles should be alive before collection"
    );
    let stats = gc::collect();
    assert!(stats.freed >= 3000 * 5, "freed only {}", stats.freed);
    let after = gc::live_objects();
    assert!(after < baseline + 200, "baseline {baseline}, after {after}");
}

#[test]
fn keeps_reachable_data_through_collections() {
    let mut vm = quiet_vm();
    run(&mut vm, LIVE);
    gc::collect();
    run(&mut vm, LIVE);
}
