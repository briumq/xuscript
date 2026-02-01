#![allow(dead_code)]
use crate::Value;
use crate::{Flow, Runtime};
use crate::vm::{Handler, IterState, Pending};

pub(crate) fn unwind_to_finally(
    rt: &mut Runtime,
    handlers: &mut Vec<Handler>,
    stack: &mut Vec<Value>,
) -> Option<usize> {
    while let Some(h) = handlers.last() {
        if let Some(fin) = h.finally_ip {
            restore_to_handler(rt, h, stack);
            return Some(fin);
        }
        restore_to_handler(rt, h, stack);
        let _ = handlers.pop();
    }
    None
}

fn restore_to_handler(rt: &mut Runtime, h: &Handler, stack: &mut Vec<Value>) {
    stack.truncate(h.stack_len);
    rt.env.pop_to(h.env_depth);
}

pub(crate) fn dispatch_throw(
    rt: &mut Runtime,
    handlers: &mut Vec<Handler>,
    stack: &mut Vec<Value>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &Option<Value>,
) -> Option<usize> {
    let tv = (*thrown)?;
    while let Some(h) = handlers.last_mut() {
        stack.truncate(h.stack_len);
        iters.truncate(h.iter_len);
        rt.env.pop_to(h.env_depth);
        if let Some(catch_ip) = h.catch_ip {
            h.catch_ip = None;
            return Some(catch_ip);
        }
        if let Some(fin) = h.finally_ip {
            *pending = Some(Pending::Throw(tv));
            return Some(fin);
        }
        let _ = handlers.pop();
    }
    None
}

pub(crate) fn throw_value(
    rt: &mut Runtime,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    stack: &mut Vec<Value>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    v: Value,
) -> Option<Flow> {
    *thrown = Some(v);
    if let Some(next_ip) = dispatch_throw(rt, handlers, stack, iters, pending, thrown) {
        *ip = next_ip;
        return None;
    }
    Some(Flow::Throw(thrown.take().unwrap()))
}

