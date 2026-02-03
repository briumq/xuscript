use super::super::Runtime;
use crate::Value;
use libc::{getrusage, rusage, RUSAGE_SELF};

pub fn builtin_time_unix(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("time_unix expects 0 arguments".into());
    }
    Ok(Value::from_i64(rt.clock_unix_secs()))
}

pub fn builtin_time_millis(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("time_millis expects 0 arguments".into());
    }
    Ok(Value::from_i64(rt.clock_unix_millis()))
}

pub fn builtin_mono_micros(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("mono_micros expects 0 arguments".into());
    }
    Ok(Value::from_i64(rt.clock_mono_micros()))
}

pub fn builtin_mono_nanos(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("mono_nanos expects 0 arguments".into());
    }
    Ok(Value::from_i64(rt.clock_mono_nanos()))
}

pub fn builtin_rand(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() > 1 {
        return Err("rand expects 0 or 1 argument".into());
    }
    let raw = rt.rng_next_u64();
    if let Some(upper) = args.first() {
        let u = if upper.is_int() {
            upper.as_i64()
        } else if upper.is_f64() {
            upper.as_f64() as i64
        } else {
            return Err("rand upper bound must be number".into());
        };
        if u <= 0 {
            return Err("rand upper bound must be > 0".into());
        }
        Ok(Value::from_i64((raw % (u as u64)) as i64))
    } else {
        Ok(Value::from_i64(raw as i64))
    }
}

pub fn builtin_os_args(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("os_args expects 0 arguments".into());
    }
    let items = rt
        .args
        .iter()
        .map(|s| {
            Value::str(
                rt.heap
                    .alloc(crate::core::heap::ManagedObject::Str(s.clone().into())),
            )
        })
        .collect::<Vec<_>>();
    Ok(Value::list(
        rt.heap.alloc(crate::core::heap::ManagedObject::List(items)),
    ))
}

pub fn builtin_env_get(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("env_get expects 1 argument".into());
    }
    let key = if args[0].get_tag() == crate::core::value::TAG_STR {
        if let crate::core::heap::ManagedObject::Str(s) = rt.heap.get(args[0].as_obj_id()) {
            s.as_str().to_string()
        } else {
            return Err("env_get expects string".into());
        }
    } else {
        return Err("env_get expects string".into());
    };
    match std::env::var(&key) {
        Ok(val) => Ok(Value::str(
            rt.heap
                .alloc(crate::core::heap::ManagedObject::Str(val.into())),
        )),
        Err(_) => Ok(Value::str(
            rt.heap
                .alloc(crate::core::heap::ManagedObject::Str("".into())),
        )),
    }
}

pub fn builtin_process_rss(_rt: &mut Runtime, _args: &[Value]) -> Result<Value, String> {
    let mut usage = rusage {
        ru_utime: libc::timeval { tv_sec: 0, tv_usec: 0 },
        ru_stime: libc::timeval { tv_sec: 0, tv_usec: 0 },
        ru_maxrss: 0,
        ru_ixrss: 0,
        ru_idrss: 0,
        ru_isrss: 0,
        ru_minflt: 0,
        ru_majflt: 0,
        ru_nswap: 0,
        ru_inblock: 0,
        ru_oublock: 0,
        ru_msgsnd: 0,
        ru_msgrcv: 0,
        ru_nsignals: 0,
        ru_nvcsw: 0,
        ru_nivcsw: 0,
    };
    unsafe {
        let _ = getrusage(RUSAGE_SELF, &mut usage);
    }
    let rss = usage.ru_maxrss as i64;
    Ok(Value::from_i64(rss))
}

pub fn builtin_heap_stats(rt: &mut Runtime, _args: &[Value]) -> Result<Value, String> {
    let stats = rt.heap.memory_stats();
    rt.write_output(&stats);
    rt.write_output("\n");
    // Print type sizes
    rt.write_output(&format!(
        "Type sizes: Text={}, DictKey={}, Value={}, ManagedObject={}\n",
        std::mem::size_of::<crate::Text>(),
        std::mem::size_of::<crate::core::value::DictKey>(),
        std::mem::size_of::<Value>(),
        std::mem::size_of::<crate::core::heap::ManagedObject>()
    ));
    Ok(Value::VOID)
}

pub fn builtin_assert(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() < 1 || args.len() > 2 {
        return Err("__builtin_assert expects 1 or 2 arguments".into());
    }
    let cond = &args[0];
    if !cond.is_bool() {
        return Err("__builtin_assert expects bool".into());
    }
    if !cond.as_bool() {
        let msg = if args.len() == 2 {
            super::super::util::value_to_string(&args[1], &rt.heap)
        } else {
            "Assertion failed".to_string()
        };
        return Err(msg);
    }
    Ok(Value::VOID)
}

pub fn builtin_assert_eq(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err("__builtin_assert_eq expects 2 or 3 arguments".into());
    }
    let a = &args[0];
    let b = &args[1];
    if !rt.values_equal(a, b) {
        let msg = if args.len() == 3 {
            super::super::util::value_to_string(&args[2], &rt.heap)
        } else {
            let sa = super::super::util::value_to_string(a, &rt.heap);
            let sb = super::super::util::value_to_string(b, &rt.heap);
            format!("Assertion failed: {} != {}", sa, sb)
        };
        return Err(msg);
    }
    Ok(Value::VOID)
}

pub fn builtin_builder_new(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("builder_new expects 0 arguments".into());
    }
    Ok(Value::builder(
        rt.heap
            .alloc(crate::core::heap::ManagedObject::Builder(String::new())),
    ))
}



pub fn builtin_builder_new_with_capacity(
    rt: &mut Runtime,
    args: &[Value],
) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("builder_new_cap expects 1 argument".into());
    }
    let v = &args[0];
    let cap = if v.is_int() && v.as_i64() >= 0 {
        v.as_i64() as usize
    } else if v.is_f64() && v.as_f64() >= 0.0 {
        v.as_f64() as usize
    } else {
        return Err("builder_new_cap expects non-negative number".into());
    };
    Ok(Value::builder(rt.heap.alloc(
        crate::core::heap::ManagedObject::Builder(String::with_capacity(cap)),
    )))
}

pub fn builtin_builder_push(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("builder_push expects 2 arguments".into());
    }
    let id = if args[0].get_tag() == crate::core::value::TAG_BUILDER {
        args[0].as_obj_id()
    } else {
        return Err("builder_push first arg must be builder".into());
    };

    let v = &args[1];
    if v.is_void() {
        if let crate::core::heap::ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
            sb.push_str("()");
        }
    } else if v.is_bool() {
        let s = if v.as_bool() { "true" } else { "false" };
        if let crate::core::heap::ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
            sb.push_str(s);
        }
    } else if v.is_int() {
        let mut buf = itoa::Buffer::new();
        let digits = buf.format(v.as_i64());
        if let crate::core::heap::ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
            sb.push_str(digits);
        }
    } else if v.is_f64() {
        let f = v.as_f64();
        let s = if f.fract() == 0.0 {
            let mut buf = itoa::Buffer::new();
            buf.format(f as i64).to_string()
        } else {
            f.to_string()
        };
        if let crate::core::heap::ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
            sb.push_str(&s);
        }
    } else if v.get_tag() == crate::core::value::TAG_STR {
        // Optimize: get string pointer and length first, then push
        let str_id = v.as_obj_id();
        let (ptr, len) = if let crate::core::heap::ManagedObject::Str(s) = rt.heap.get(str_id) {
            (s.as_str().as_ptr(), s.as_str().len())
        } else {
            (std::ptr::null(), 0)
        };
        if !ptr.is_null() {
            // SAFETY: We're accessing the same heap, and the string won't be moved
            // during this operation since we're not allocating
            let str_slice = unsafe { std::slice::from_raw_parts(ptr, len) };
            let str_ref = unsafe { std::str::from_utf8_unchecked(str_slice) };
            if let crate::core::heap::ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
                sb.push_str(str_ref);
            }
        }
    } else if v.get_tag() == crate::core::value::TAG_BUILDER {
        let s = if let crate::core::heap::ManagedObject::Builder(s) = rt.heap.get(v.as_obj_id()) {
            s.clone()
        } else {
            String::new()
        };
        if let crate::core::heap::ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
            sb.push_str(&s);
        }
    } else {
        let s = super::super::util::value_to_string(v, &rt.heap);
        if let crate::core::heap::ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
            sb.push_str(&s);
        }
    }

    Ok(Value::VOID)
}

pub fn builtin_builder_finalize(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("builder_finalize expects 1 argument".into());
    }
    let v = &args[0];
    if v.get_tag() == crate::core::value::TAG_BUILDER {
        let id = v.as_obj_id();
        if let crate::core::heap::ManagedObject::Builder(s) = rt.heap.get(id) {
            Ok(Value::str(
                rt.heap
                    .alloc(crate::core::heap::ManagedObject::Str(s.clone().into())),
            ))
        } else {
            Err("Not a builder".into())
        }
    } else {
        Err("builder_finalize expects builder".into())
    }
}
