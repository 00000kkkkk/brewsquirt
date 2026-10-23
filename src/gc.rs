use crate::object::{
    Closure, IterState, MapKey, Obj, ObjKind, PromiseState, Reaction, SavedFrame, UpCell, Upvalue,
};
use crate::value::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static MIN_THRESHOLD: AtomicUsize = AtomicUsize::new(200_000);
static THRESHOLD: AtomicUsize = AtomicUsize::new(200_000);

thread_local! {
    static HEAP: RefCell<Vec<*const Obj>> = const { RefCell::new(Vec::new()) };
    static ENABLED: Cell<bool> = const { Cell::new(true) };
}

pub(crate) fn register(o: &Rc<Obj>) {
    HEAP.with(|h| {
        let mut h = h.borrow_mut();
        o.heap_slot.set(h.len() as u32);
        h.push(Rc::as_ptr(o));
    });
    ALLOCS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn unregister(o: &Obj) {
    let slot = o.heap_slot.get();
    if slot == u32::MAX {
        return;
    }
    let _ = HEAP.try_with(|h| {
        if let Ok(mut h) = h.try_borrow_mut() {
            let slot = slot as usize;
            if slot < h.len() && std::ptr::eq(h[slot], o) {
                h.swap_remove(slot);
                if let Some(&moved) = h.get(slot) {
                    unsafe { (*moved).heap_slot.set(slot as u32) };
                }
            }
        }
    });
}

#[inline]
pub fn due() -> bool {
    ALLOCS.load(Ordering::Relaxed) >= THRESHOLD.load(Ordering::Relaxed)
}

pub fn configure_from_env() {
    if let Some(n) = std::env::var("JSQ_GC_THRESHOLD")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        let n = n.max(1);
        MIN_THRESHOLD.store(n, Ordering::Relaxed);
        THRESHOLD.store(n, Ordering::Relaxed);
    }
}

pub fn set_enabled(on: bool) {
    ENABLED.with(|e| e.set(on));
}

pub fn live_objects() -> usize {
    HEAP.with(|h| h.borrow().len())
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub scanned: usize,
    pub freed: usize,
    pub live: usize,
}

enum Node {
    Obj(Rc<Obj>),
    Closure(Rc<Closure>),
    Cell(Rc<UpCell>),
}

impl Node {
    fn strong(&self) -> usize {
        match self {
            Node::Obj(o) => Rc::strong_count(o),
            Node::Closure(c) => Rc::strong_count(c),
            Node::Cell(c) => Rc::strong_count(c),
        }
    }
}

enum Edge<'a> {
    Obj(&'a Rc<Obj>),
    Closure(&'a Rc<Closure>),
    Cell(&'a Rc<UpCell>),
}

fn slot_of(e: &Edge<'_>) -> u32 {
    match e {
        Edge::Obj(o) => o.gc_slot.get(),
        Edge::Closure(c) => c.gc_slot.get(),
        Edge::Cell(c) => c.gc_slot.get(),
    }
}

impl Node {
    fn reset_slot(&self) {
        match self {
            Node::Obj(o) => o.gc_slot.set(u32::MAX),
            Node::Closure(c) => c.gc_slot.set(u32::MAX),
            Node::Cell(c) => c.gc_slot.set(u32::MAX),
        }
    }
}

fn value_edge(v: &Value, f: &mut dyn FnMut(Edge<'_>)) {
    if let Value::Object(o) = v {
        f(Edge::Obj(o));
    }
}

fn reaction_edges(r: &Reaction, f: &mut dyn FnMut(Edge<'_>)) {
    match r {
        Reaction::Then {
            on_fulfilled,
            on_rejected,
            derived,
        } => {
            value_edge(on_fulfilled, f);
            value_edge(on_rejected, f);
            if let Some(d) = derived {
                f(Edge::Obj(d));
            }
        }
        Reaction::Forward(p) | Reaction::Resume(p) => f(Edge::Obj(p)),
    }
}

fn saved_edges(sf: &SavedFrame, f: &mut dyn FnMut(Edge<'_>)) {
    f(Edge::Closure(&sf.closure));
    for v in &sf.stack {
        value_edge(v, f);
    }
    for (c, _) in &sf.upvalues {
        f(Edge::Cell(c));
    }
}

fn obj_edges(o: &Obj, f: &mut dyn FnMut(Edge<'_>)) {
    if let Ok(props) = o.props.try_borrow() {
        for v in props.values() {
            value_edge(v, f);
        }
    }
    if let Ok(proto) = o.proto.try_borrow() {
        if let Some(p) = &*proto {
            f(Edge::Obj(p));
        }
    }
    match &o.kind {
        ObjKind::Ordinary
        | ObjKind::Error
        | ObjKind::Native(_)
        | ObjKind::RegExp(_)
        | ObjKind::Date(_)
        | ObjKind::WeakSet(_)
        | ObjKind::WeakRef(_)
        | ObjKind::ArrayBuffer(_)
        | ObjKind::Namespace
        | ObjKind::Primitive(_) => {}
        ObjKind::Typed(t) | ObjKind::DataView(t) => f(Edge::Obj(&t.buffer)),
        ObjKind::Global => {}
        ObjKind::Proxy(p) => {
            if let Ok(t) = p.target.try_borrow() {
                if let Some(t) = &*t {
                    f(Edge::Obj(t));
                }
            }
            if let Ok(h) = p.handler.try_borrow() {
                if let Some(h) = &*h {
                    f(Edge::Obj(h));
                }
            }
        }
        ObjKind::WeakMap(m) => {
            if let Ok(m) = m.try_borrow() {
                for (_, v) in m.values() {
                    value_edge(v, f);
                }
            }
        }
        ObjKind::Array(a) => {
            if let Ok(a) = a.try_borrow() {
                for v in a.iter() {
                    value_edge(v, f);
                }
            }
        }
        ObjKind::Function(c) => f(Edge::Closure(c)),
        ObjKind::Arguments(m) => {
            if let Ok(cells) = m.cells.try_borrow() {
                for c in cells.iter().flatten() {
                    f(Edge::Cell(c));
                }
            }
        }
        ObjKind::Bound(b) => {
            value_edge(&b.target, f);
            value_edge(&b.this, f);
            for v in &b.args {
                value_edge(v, f);
            }
        }
        ObjKind::Map(m) => {
            if let Ok(m) = m.try_borrow() {
                for (k, v) in m.iter() {
                    if let MapKey::Obj(o) = k {
                        f(Edge::Obj(o));
                    }
                    value_edge(v, f);
                }
            }
        }
        ObjKind::Set(s) => {
            if let Ok(s) = s.try_borrow() {
                for k in s.iter() {
                    if let MapKey::Obj(o) = k {
                        f(Edge::Obj(o));
                    }
                }
            }
        }
        ObjKind::Iter(st) => {
            if let Ok(st) = st.try_borrow() {
                match &*st {
                    IterState::Array(o, _, _)
                    | IterState::MapEntries(o, _)
                    | IterState::MapKeys(o, _)
                    | IterState::MapValues(o, _)
                    | IterState::SetValues(o, _)
                    | IterState::SetEntries(o, _)
                    | IterState::Typed(o, _, _) => f(Edge::Obj(o)),
                    IterState::Str(..) => {}
                    IterState::List(items, _) => {
                        for v in items {
                            value_edge(v, f);
                        }
                    }
                    IterState::ForIn(obj, items, _) => {
                        value_edge(obj, f);
                        for v in items {
                            value_edge(v, f);
                        }
                    }
                    IterState::Protocol(a, b) => {
                        value_edge(a, f);
                        value_edge(b, f);
                    }
                }
            }
        }
        ObjKind::Accessor(acc) => {
            if let Ok(g) = acc.get.try_borrow() {
                value_edge(&g, f);
            }
            if let Ok(s) = acc.set.try_borrow() {
                value_edge(&s, f);
            }
        }
        ObjKind::Promise(p) => {
            if let Ok(p) = p.try_borrow() {
                match &p.state {
                    PromiseState::Fulfilled(v) | PromiseState::Rejected(v) => value_edge(v, f),
                    PromiseState::Pending => {}
                }
                for r in &p.reactions {
                    reaction_edges(r, f);
                }
            }
        }
        ObjKind::Coroutine(co) => {
            if let Ok(saved) = co.saved.try_borrow() {
                if let Some(sf) = &*saved {
                    saved_edges(sf, f);
                }
            }
            if let Some(p) = &co.promise {
                f(Edge::Obj(p));
            }
            if let Ok(q) = co.queue.try_borrow() {
                for r in q.iter() {
                    value_edge(&r.value, f);
                    f(Edge::Obj(&r.promise));
                }
            }
        }
    }
}

fn node_edges(n: &Node, f: &mut dyn FnMut(Edge<'_>)) {
    match n {
        Node::Obj(o) => obj_edges(o, f),
        Node::Closure(c) => {
            for u in c.upvalues.iter() {
                f(Edge::Cell(u));
            }
            if let Ok(h) = c.home.try_borrow() {
                if let Some(h) = &*h {
                    f(Edge::Obj(h));
                }
            }
            if let Ok(t) = c.new_target.try_borrow() {
                if let Some(t) = &*t {
                    value_edge(t, f);
                }
            }
        }
        Node::Cell(c) => {
            if let Ok(u) = c.value.try_borrow() {
                if let Upvalue::Closed(v) = &*u {
                    value_edge(v, f);
                }
            }
        }
    }
}

#[derive(Default)]
struct Graveyard {
    values: Vec<Value>,
    objs: Vec<Rc<Obj>>,
    frames: Vec<SavedFrame>,
    maps: Vec<Box<dyn std::any::Any>>,
}

fn clear(n: &Node, grave: &mut Graveyard) {
    match n {
        Node::Obj(o) => {
            if let Ok(mut props) = o.props.try_borrow_mut() {
                let taken = std::mem::take(&mut *props);
                grave.values.extend(taken.into_values());
            }
            if let Ok(mut proto) = o.proto.try_borrow_mut() {
                if let Some(p) = proto.take() {
                    grave.objs.push(p);
                }
            }
            match &o.kind {
                ObjKind::Array(a) => {
                    if let Ok(mut a) = a.try_borrow_mut() {
                        grave.values.append(&mut a);
                    }
                }
                ObjKind::Map(m) => {
                    if let Ok(mut m) = m.try_borrow_mut() {
                        grave.maps.push(Box::new(std::mem::take(&mut *m)));
                    }
                }
                ObjKind::Set(s) => {
                    if let Ok(mut s) = s.try_borrow_mut() {
                        grave.maps.push(Box::new(std::mem::take(&mut *s)));
                    }
                }
                ObjKind::Iter(st) => {
                    if let Ok(mut st) = st.try_borrow_mut() {
                        let old = std::mem::replace(&mut *st, IterState::List(Vec::new(), 0));
                        grave.maps.push(Box::new(old));
                    }
                }
                ObjKind::Accessor(acc) => {
                    if let Ok(mut g) = acc.get.try_borrow_mut() {
                        grave.values.push(std::mem::take(&mut *g));
                    }
                    if let Ok(mut s) = acc.set.try_borrow_mut() {
                        grave.values.push(std::mem::take(&mut *s));
                    }
                }
                ObjKind::Promise(p) => {
                    if let Ok(mut p) = p.try_borrow_mut() {
                        let state = std::mem::replace(&mut p.state, PromiseState::Pending);
                        if let PromiseState::Fulfilled(v) | PromiseState::Rejected(v) = state {
                            grave.values.push(v);
                        }
                        grave.maps.push(Box::new(std::mem::take(&mut p.reactions)));
                    }
                }
                ObjKind::WeakMap(m) => {
                    if let Ok(mut m) = m.try_borrow_mut() {
                        grave.maps.push(Box::new(std::mem::take(&mut *m)));
                    }
                }
                ObjKind::Proxy(p) => {
                    if let Ok(mut t) = p.target.try_borrow_mut() {
                        if let Some(t) = t.take() {
                            grave.objs.push(t);
                        }
                    }
                    if let Ok(mut h) = p.handler.try_borrow_mut() {
                        if let Some(h) = h.take() {
                            grave.objs.push(h);
                        }
                    }
                }
                ObjKind::Coroutine(co) => {
                    if let Ok(mut saved) = co.saved.try_borrow_mut() {
                        if let Some(sf) = saved.take() {
                            grave.frames.push(sf);
                        }
                    }
                    if let Ok(mut q) = co.queue.try_borrow_mut() {
                        grave.maps.push(Box::new(std::mem::take(&mut *q)));
                    }
                }
                _ => {}
            }
        }
        Node::Closure(c) => {
            if let Ok(mut h) = c.home.try_borrow_mut() {
                if let Some(h) = h.take() {
                    grave.objs.push(h);
                }
            }
            if let Ok(mut t) = c.new_target.try_borrow_mut() {
                if let Some(t) = t.take() {
                    grave.values.push(t);
                }
            }
        }
        Node::Cell(c) => {
            if let Ok(mut u) = c.value.try_borrow_mut() {
                if let Upvalue::Closed(v) = &mut *u {
                    grave.values.push(std::mem::take(v));
                }
            }
        }
    }
}

pub fn collect() -> Stats {
    ALLOCS.store(0, Ordering::Relaxed);
    if !ENABLED.with(|e| e.get()) {
        return Stats::default();
    }
    let objs: Vec<Rc<Obj>> = HEAP.with(|h| {
        h.borrow()
            .iter()
            .map(|&p| unsafe {
                Rc::increment_strong_count(p);
                Rc::from_raw(p)
            })
            .collect()
    });
    let obj_count = objs.len();
    let mut nodes: Vec<Node> = Vec::with_capacity(obj_count + obj_count / 2);
    let mut incoming: Vec<u32> = Vec::with_capacity(obj_count + obj_count / 2);
    for (i, o) in objs.into_iter().enumerate() {
        o.gc_slot.set(i as u32);
        nodes.push(Node::Obj(o));
        incoming.push(0);
    }
    let mut fresh: Vec<Node> = Vec::new();
    let mut i = 0;
    while i < nodes.len() {
        let base = nodes.len();
        node_edges(&nodes[i], &mut |e| {
            let slot = slot_of(&e);
            if slot != u32::MAX {
                incoming[slot as usize] += 1;
                return;
            }
            let next = (base + fresh.len()) as u32;
            match e {
                Edge::Obj(_) => return,
                Edge::Closure(c) => {
                    c.gc_slot.set(next);
                    fresh.push(Node::Closure(c.clone()));
                }
                Edge::Cell(c) => {
                    c.gc_slot.set(next);
                    fresh.push(Node::Cell(c.clone()));
                }
            }
            incoming.push(1);
        });
        nodes.append(&mut fresh);
        i += 1;
    }

    let mut marked = vec![false; nodes.len()];
    let mut stack: Vec<u32> = Vec::new();
    for (idx, n) in nodes.iter().enumerate() {
        if n.strong() > incoming[idx] as usize + 1 {
            marked[idx] = true;
            stack.push(idx as u32);
        }
    }
    while let Some(idx) = stack.pop() {
        node_edges(&nodes[idx as usize], &mut |e| {
            let slot = slot_of(&e);
            if slot != u32::MAX && !marked[slot as usize] {
                marked[slot as usize] = true;
                stack.push(slot);
            }
        });
    }

    let mut grave = Graveyard::default();
    let mut freed = 0;
    for (idx, n) in nodes.iter().enumerate() {
        if !marked[idx] {
            if idx < obj_count {
                freed += 1;
            }
            clear(n, &mut grave);
        }
    }
    for n in &nodes {
        n.reset_slot();
    }
    drop(nodes);
    drop(grave);
    let live = obj_count - freed;
    THRESHOLD.store(
        (live * 2).max(MIN_THRESHOLD.load(Ordering::Relaxed)),
        Ordering::Relaxed,
    );
    Stats {
        scanned: obj_count,
        freed,
        live,
    }
}
