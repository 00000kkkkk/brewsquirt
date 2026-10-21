use crate::object::{
    Bound, Closure, CoState, Coroutine, Obj, ObjKind, PromiseData, PromiseState, Reaction,
    SavedFrame, Upvalue,
};
use crate::value::Value;
use crate::vm::{ErrorKind, Frame, Handler, Vm};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

pub(crate) enum Job {
    Reaction(Reaction, Value, bool),
    Thenable(Rc<Obj>, Value, Value),
    Callback(Value, Vec<Value>),
}

pub(crate) struct Timer {
    pub id: u64,
    pub callback: Value,
    pub args: Vec<Value>,
    pub interval: Option<u128>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Signal {
    Yield,
    Await,
}

pub enum Step {
    Yield(Value),
    Await(Value),
    Done(Value),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResumeMode {
    Next,
    Throw,
    Return,
}

impl Vm {
    pub(crate) fn start_coroutine(
        &mut self,
        f: &Value,
        c: Rc<Closure>,
        this: Value,
        args: &[Value],
    ) -> Result<Value, Value> {
        let mark = self.stack.len();
        self.stack.push(f.clone());
        self.stack.push(this);
        self.stack.extend_from_slice(args);
        if let Err(e) = self.push_frame(c.clone(), args.len(), false) {
            self.stack.truncate(mark);
            return Err(e);
        }
        let frame = self.frames.pop().expect("coroutine frame");
        let stack = self.stack.split_off(mark);
        let saved = SavedFrame {
            closure: frame.closure,
            ip: 0,
            stack,
            handlers: Vec::new(),
            upvalues: Vec::new(),
            started: false,
        };
        if c.proto.is_generator {
            let is_async = c.proto.is_async;
            let co = Coroutine {
                saved: RefCell::new(Some(saved)),
                state: Cell::new(CoState::Suspended),
                promise: None,
                is_generator: true,
                is_async,
                queue: RefCell::new(Default::default()),
                awaiting: Cell::new(false),
            };
            let proto = if is_async {
                self.realm.async_generator_proto.clone()
            } else {
                self.realm.generator_proto.clone()
            };
            return Ok(Value::Object(Obj::new(
                ObjKind::Coroutine(Box::new(co)),
                Some(proto),
            )));
        }
        let promise = self.new_promise();
        let co = Obj::new(
            ObjKind::Coroutine(Box::new(Coroutine {
                saved: RefCell::new(Some(saved)),
                state: Cell::new(CoState::Suspended),
                promise: Some(promise.clone()),
                is_generator: false,
                is_async: true,
                queue: RefCell::new(Default::default()),
                awaiting: Cell::new(false),
            })),
            None,
        );
        self.async_step(&co, Value::Undefined, ResumeMode::Next);
        Ok(Value::Object(promise))
    }

    pub(crate) fn suspend_frame(&mut self, fi: usize) -> bool {
        let Some(co) = self.frames[fi].co.clone() else {
            return false;
        };
        let seg = self.frames[fi].base - 1;
        let mut handlers = Vec::new();
        while self.handlers.last().is_some_and(|h| h.frame >= fi) {
            let h = self.handlers.pop().expect("handler");
            handlers.push((h.stack_len - seg, h.catch_ip));
        }
        handlers.reverse();
        let mut upvalues = Vec::new();
        while let Some(u) = self.open_upvalues.last() {
            let idx = match &*u.borrow() {
                Upvalue::Open(i) => *i,
                Upvalue::Closed(_) => break,
            };
            if idx < seg {
                break;
            }
            let u = self.open_upvalues.pop().expect("upvalue");
            let v = self.stack[idx].clone();
            *u.borrow_mut() = Upvalue::Closed(v);
            upvalues.push((u, idx - seg));
        }
        upvalues.reverse();
        let frame = self.frames.pop().expect("frame");
        let stack = self.stack.split_off(seg);
        if let ObjKind::Coroutine(c) = &co.kind {
            *c.saved.borrow_mut() = Some(SavedFrame {
                closure: frame.closure,
                ip: frame.ip,
                stack,
                handlers,
                upvalues,
                started: true,
            });
            c.state.set(CoState::Suspended);
        }
        true
    }

    pub fn resume(
        &mut self,
        co_obj: &Rc<Obj>,
        value: Value,
        mode: ResumeMode,
    ) -> Result<Step, Value> {
        let ObjKind::Coroutine(co) = &co_obj.kind else {
            return Err(self.type_error("Resumed value is not a generator"));
        };
        match co.state.get() {
            CoState::Running => return Err(self.type_error("Generator is already running")),
            CoState::Done => {
                return match mode {
                    ResumeMode::Throw => Err(value),
                    ResumeMode::Return => Ok(Step::Done(value)),
                    ResumeMode::Next => Ok(Step::Done(Value::Undefined)),
                };
            }
            CoState::Suspended => {}
        }
        let Some(saved) = co.saved.borrow_mut().take() else {
            co.state.set(CoState::Done);
            return Ok(Step::Done(Value::Undefined));
        };
        if mode == ResumeMode::Return || (mode == ResumeMode::Throw && !saved.started) {
            co.state.set(CoState::Done);
            return if mode == ResumeMode::Throw {
                Err(value)
            } else {
                Ok(Step::Done(value))
            };
        }
        let seg = self.stack.len();
        self.stack.extend(saved.stack);
        for (u, rel) in saved.upvalues {
            let idx = seg + rel;
            let v = match &*u.borrow() {
                Upvalue::Closed(v) => v.clone(),
                Upvalue::Open(_) => Value::Undefined,
            };
            self.stack[idx] = v;
            *u.borrow_mut() = Upvalue::Open(idx);
            self.open_upvalues.push(u);
        }
        let fi = self.frames.len();
        self.frames.push(Frame {
            closure: saved.closure,
            ip: saved.ip,
            base: seg + 1,
            construct: false,
            co: Some(co_obj.clone()),
        });
        for (rel, catch_ip) in saved.handlers {
            self.handlers.push(Handler {
                frame: fi,
                stack_len: seg + rel,
                catch_ip,
            });
        }
        co.state.set(CoState::Running);
        if saved.started {
            if mode == ResumeMode::Throw {
                if let Err(e) = self.unwind(value, fi) {
                    co.state.set(CoState::Done);
                    return Err(e);
                }
            } else {
                self.stack.push(value);
            }
        }
        self.native_depth += 1;
        let r = self.run(fi);
        self.native_depth -= 1;
        let signal = self.co_signal.take();
        match r {
            Ok(v) => match signal {
                Some(Signal::Yield) => Ok(Step::Yield(v)),
                Some(Signal::Await) => Ok(Step::Await(v)),
                None => {
                    co.state.set(CoState::Done);
                    Ok(Step::Done(v))
                }
            },
            Err(e) => {
                co.state.set(CoState::Done);
                Err(e)
            }
        }
    }

    pub(crate) fn async_step(&mut self, co: &Rc<Obj>, value: Value, mode: ResumeMode) {
        if let ObjKind::Coroutine(c) = &co.kind {
            if c.is_generator && c.is_async {
                self.async_gen_resume(co, value, mode);
                return;
            }
        }
        let promise = match &co.kind {
            ObjKind::Coroutine(c) => c.promise.clone(),
            _ => None,
        };
        let Some(promise) = promise else {
            return;
        };
        match self.resume(co, value, mode) {
            Ok(Step::Await(v)) => {
                let p = self.to_promise(v);
                self.add_reaction(&p, Reaction::Resume(co.clone()));
            }
            Ok(Step::Done(v)) | Ok(Step::Yield(v)) => self.resolve_promise(&promise, v),
            Err(e) => self.reject_promise(&promise, e),
        }
    }

    pub fn async_gen_enqueue(&mut self, agen: &Rc<Obj>, value: Value, mode: ResumeMode) -> Rc<Obj> {
        let promise = self.new_promise();
        if let ObjKind::Coroutine(c) = &agen.kind {
            c.queue.borrow_mut().push_back(crate::object::AsyncRequest {
                value,
                mode,
                promise: promise.clone(),
            });
        }
        self.async_gen_drain(agen);
        promise
    }

    fn async_gen_settle(&mut self, agen: &Rc<Obj>, result: Result<(Value, bool), Value>) {
        let ObjKind::Coroutine(c) = &agen.kind else {
            return;
        };
        let Some(req) = c.queue.borrow_mut().pop_front() else {
            return;
        };
        match result {
            Ok((v, done)) => {
                let r = crate::builtins::iter_result(self, v, done);
                self.resolve_promise(&req.promise, r);
            }
            Err(e) => self.reject_promise(&req.promise, e),
        }
    }

    fn async_gen_resume(&mut self, agen: &Rc<Obj>, value: Value, mode: ResumeMode) {
        let r = self.resume(agen, value, mode);
        let ObjKind::Coroutine(c) = &agen.kind else {
            return;
        };
        match r {
            Ok(Step::Await(v)) => {
                c.awaiting.set(true);
                let p = self.to_promise(v);
                self.add_reaction(&p, Reaction::Resume(agen.clone()));
                return;
            }
            Ok(Step::Yield(v)) => self.async_gen_settle(agen, Ok((v, false))),
            Ok(Step::Done(v)) => self.async_gen_settle(agen, Ok((v, true))),
            Err(e) => self.async_gen_settle(agen, Err(e)),
        }
        c.awaiting.set(false);
        self.async_gen_drain(agen);
    }

    fn async_gen_drain(&mut self, agen: &Rc<Obj>) {
        loop {
            let ObjKind::Coroutine(c) = &agen.kind else {
                return;
            };
            if c.awaiting.get() || c.state.get() == CoState::Running {
                return;
            }
            let front = c.queue.borrow().front().map(|r| (r.value.clone(), r.mode));
            let Some((value, mode)) = front else {
                return;
            };
            if c.state.get() == CoState::Done {
                match mode {
                    ResumeMode::Throw => self.async_gen_settle(agen, Err(value)),
                    ResumeMode::Return => self.async_gen_settle(agen, Ok((value, true))),
                    ResumeMode::Next => self.async_gen_settle(agen, Ok((Value::Undefined, true))),
                }
                continue;
            }
            self.async_gen_resume(agen, value, mode);
            return;
        }
    }

    pub fn new_promise(&self) -> Rc<Obj> {
        Obj::new(
            ObjKind::Promise(Box::new(RefCell::new(PromiseData::default()))),
            Some(self.realm.promise_proto.clone()),
        )
    }

    pub fn is_promise(v: &Value) -> bool {
        matches!(v, Value::Object(o) if matches!(o.kind, ObjKind::Promise(_)))
    }

    pub fn to_promise(&mut self, v: Value) -> Rc<Obj> {
        if let Value::Object(o) = &v {
            if matches!(o.kind, ObjKind::Promise(_)) {
                return o.clone();
            }
        }
        let p = self.new_promise();
        self.resolve_promise(&p, v);
        p
    }

    fn is_pending(p: &Obj) -> bool {
        match &p.kind {
            ObjKind::Promise(d) => matches!(d.borrow().state, PromiseState::Pending),
            _ => false,
        }
    }

    pub fn resolve_promise(&mut self, p: &Rc<Obj>, v: Value) {
        if !Self::is_pending(p) {
            return;
        }
        if let Value::Object(o) = &v {
            if Rc::ptr_eq(o, p) {
                let e = self.type_error("Chaining cycle detected for promise");
                self.reject_promise(p, e);
                return;
            }
            if matches!(o.kind, ObjKind::Promise(_)) {
                let o = o.clone();
                self.add_reaction(&o, Reaction::Forward(p.clone()));
                return;
            }
            match self.get_prop(&v, "then") {
                Err(e) => {
                    self.reject_promise(p, e);
                    return;
                }
                Ok(then) if then.is_callable() => {
                    self.microtasks
                        .push_back(Job::Thenable(p.clone(), v.clone(), then));
                    return;
                }
                Ok(_) => {}
            }
        }
        self.settle(p, PromiseState::Fulfilled(v));
    }

    pub fn reject_promise(&mut self, p: &Rc<Obj>, e: Value) {
        self.settle(p, PromiseState::Rejected(e));
    }

    fn settle(&mut self, p: &Rc<Obj>, state: PromiseState) {
        let ObjKind::Promise(data) = &p.kind else {
            return;
        };
        let mut d = data.borrow_mut();
        if !matches!(d.state, PromiseState::Pending) {
            return;
        }
        let (arg, rejected) = match &state {
            PromiseState::Fulfilled(v) => (v.clone(), false),
            PromiseState::Rejected(e) => (e.clone(), true),
            PromiseState::Pending => return,
        };
        d.state = state;
        let reactions = std::mem::take(&mut d.reactions);
        let unhandled = rejected && !d.handled;
        drop(d);
        if unhandled {
            self.pending_rejections.push(p.clone());
        }
        for r in reactions {
            self.microtasks
                .push_back(Job::Reaction(r, arg.clone(), rejected));
        }
    }

    pub fn add_reaction(&mut self, p: &Rc<Obj>, r: Reaction) {
        let ObjKind::Promise(data) = &p.kind else {
            return;
        };
        let settled = {
            let mut d = data.borrow_mut();
            d.handled = true;
            match &d.state {
                PromiseState::Pending => None,
                PromiseState::Fulfilled(v) => Some((v.clone(), false)),
                PromiseState::Rejected(e) => Some((e.clone(), true)),
            }
        };
        match settled {
            None => {
                if let ObjKind::Promise(data) = &p.kind {
                    data.borrow_mut().reactions.push(r);
                }
            }
            Some((arg, rejected)) => {
                if rejected {
                    self.pending_rejections.retain(|x| !Rc::ptr_eq(x, p));
                }
                self.microtasks.push_back(Job::Reaction(r, arg, rejected));
            }
        }
    }

    pub fn promise_then(
        &mut self,
        p: &Rc<Obj>,
        on_fulfilled: Value,
        on_rejected: Value,
    ) -> Rc<Obj> {
        let derived = self.new_promise();
        self.add_reaction(
            p,
            Reaction::Then {
                on_fulfilled,
                on_rejected,
                derived: Some(derived.clone()),
            },
        );
        derived
    }

    pub fn resolving_functions(&self, p: &Rc<Obj>) -> (Value, Value) {
        let make = |target: &Value| {
            Value::Object(Obj::new(
                ObjKind::Bound(Box::new(Bound {
                    target: target.clone(),
                    this: Value::Object(p.clone()),
                    args: Vec::new(),
                })),
                Some(self.realm.function_proto.clone()),
            ))
        };
        (
            make(&self.promise_resolve_fn),
            make(&self.promise_reject_fn),
        )
    }

    pub fn queue_microtask(&mut self, f: Value, args: Vec<Value>) {
        self.microtasks.push_back(Job::Callback(f, args));
    }

    fn run_job(&mut self, job: Job) -> Result<(), Value> {
        match job {
            Job::Reaction(reaction, arg, rejected) => match reaction {
                Reaction::Then {
                    on_fulfilled,
                    on_rejected,
                    derived,
                } => {
                    let handler = if rejected { on_rejected } else { on_fulfilled };
                    if handler.is_callable() {
                        let r = self.call_value(&handler, Value::Undefined, &[arg]);
                        if let Some(d) = derived {
                            match r {
                                Ok(v) => self.resolve_promise(&d, v),
                                Err(e) => self.reject_promise(&d, e),
                            }
                        }
                    } else if let Some(d) = derived {
                        if rejected {
                            self.reject_promise(&d, arg);
                        } else {
                            self.resolve_promise(&d, arg);
                        }
                    }
                }
                Reaction::Forward(target) => {
                    if rejected {
                        self.reject_promise(&target, arg);
                    } else {
                        self.resolve_promise(&target, arg);
                    }
                }
                Reaction::Resume(co) => {
                    let mode = if rejected {
                        ResumeMode::Throw
                    } else {
                        ResumeMode::Next
                    };
                    self.async_step(&co, arg, mode);
                }
            },
            Job::Thenable(p, thenable, then) => {
                let (resolve, reject) = self.resolving_functions(&p);
                if let Err(e) = self.call_value(&then, thenable, &[resolve, reject]) {
                    self.reject_promise(&p, e);
                }
            }
            Job::Callback(f, args) => {
                self.call_value(&f, Value::Undefined, &args)?;
            }
        }
        Ok(())
    }

    pub fn run_microtasks(&mut self) -> Result<(), Value> {
        while let Some(job) = self.microtasks.pop_front() {
            self.run_job(job)?;
        }
        Ok(())
    }

    fn check_rejections(&mut self) -> Result<(), Value> {
        while !self.pending_rejections.is_empty() {
            let p = self.pending_rejections.remove(0);
            if let ObjKind::Promise(d) = &p.kind {
                let d = d.borrow();
                if !d.handled {
                    if let PromiseState::Rejected(e) = &d.state {
                        self.rejection = true;
                        return Err(e.clone());
                    }
                }
            }
        }
        Ok(())
    }

    fn next_timer_seq(&mut self) -> u64 {
        self.timer_seq += 1;
        self.timer_seq
    }

    pub fn add_timer(
        &mut self,
        callback: Value,
        delay_ms: f64,
        args: Vec<Value>,
        repeat: bool,
    ) -> u64 {
        let delay_ms = if delay_ms.is_finite() {
            delay_ms.max(0.0)
        } else {
            0.0
        };
        let mut delay = (delay_ms * 1_000_000.0) as u128;
        if repeat {
            delay = delay.max(1_000_000);
        }
        let id = self.next_timer_seq();
        let key = (self.start.elapsed().as_nanos() + delay, id);
        self.timers.insert(
            key,
            crate::event_loop::Timer {
                id,
                callback,
                args,
                interval: repeat.then_some(delay),
            },
        );
        self.timer_keys.insert(id, key);
        id
    }

    pub fn clear_timer(&mut self, id: u64) {
        if let Some(key) = self.timer_keys.remove(&id) {
            self.timers.remove(&key);
        }
    }

    fn run_next_timer(&mut self) -> Result<bool, Value> {
        let Some(((due, _), timer)) = self.timers.pop_first() else {
            return Ok(false);
        };
        self.timer_keys.remove(&timer.id);
        let now = self.start.elapsed().as_nanos();
        if due > now {
            self.flush();
            std::thread::sleep(Duration::from_nanos((due - now) as u64));
        }
        if let Some(interval) = timer.interval {
            let seq = self.next_timer_seq();
            let key = (due.max(now) + interval, seq);
            self.timer_keys.insert(timer.id, key);
            self.timers.insert(
                key,
                Timer {
                    id: timer.id,
                    callback: timer.callback.clone(),
                    args: timer.args.clone(),
                    interval: Some(interval),
                },
            );
        }
        self.call_value(&timer.callback, Value::Undefined, &timer.args)?;
        Ok(true)
    }

    pub fn run_event_loop(&mut self) -> Result<(), Value> {
        loop {
            self.run_microtasks()?;
            self.check_rejections()?;
            if !self.run_next_timer()? {
                return Ok(());
            }
        }
    }

    pub fn await_value(&mut self, v: &Value) -> Result<Value, Value> {
        let Value::Object(p) = v else {
            return Ok(v.clone());
        };
        let ObjKind::Promise(data) = &p.kind else {
            return Ok(v.clone());
        };
        data.borrow_mut().handled = true;
        self.pending_rejections.retain(|x| !Rc::ptr_eq(x, p));
        loop {
            match &data.borrow().state {
                PromiseState::Fulfilled(x) => return Ok(x.clone()),
                PromiseState::Rejected(e) => return Err(e.clone()),
                PromiseState::Pending => {}
            }
            if !self.microtasks.is_empty() {
                self.run_microtasks()?;
                continue;
            }
            if !self.run_next_timer()? {
                return Err(self.make_error(
                    ErrorKind::Error,
                    "Promise never settled: the event loop ran out of work",
                ));
            }
        }
    }
}
