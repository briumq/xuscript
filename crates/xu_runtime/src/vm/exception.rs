use crate::Value;
use crate::{Flow, Runtime};
use super::{Handler, IterState, Pending};

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
    // SAFETY: We just set *thrown = Some(v) above, so take() will return Some
    Some(Flow::Throw(thrown.take().expect("thrown value was just set")))
}
