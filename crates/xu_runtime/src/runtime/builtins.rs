use std::io::BufRead;

use super::Runtime;
use super::util::{to_f64_pair, to_i64, value_to_string};
use crate::Value;
use crate::value::{FileHandle, i64_to_text_fast};
use libc::{getrusage, rusage, RUSAGE_SELF};

pub(super) fn builtin_print(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    for a in args {
        rt.write_output(&value_to_string(a, &rt.heap));
    }
    Ok(Value::VOID)
}

pub(super) fn builtin_gen_id(rt: &mut Runtime, _args: &[Value]) -> Result<Value, String> {
    let id = rt.next_id;
    rt.next_id = rt.next_id.saturating_add(1);
    Ok(Value::from_i64(id as i64))
}

pub(super) fn builtin_gc(rt: &mut Runtime, _args: &[Value]) -> Result<Value, String> {
    rt.gc(&[]);
    // Try to release memory back to OS
    #[cfg(target_os = "linux")]
    unsafe {
        libc::malloc_trim(0);
    }
    // Force a thread yield to allow OS to reclaim memory
    std::thread::sleep(std::time::Duration::from_millis(10));
    Ok(Value::VOID)
}

pub(super) fn builtin_open(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("open expects 1 argument".into());
    }
    let path = if args[0].get_tag() == crate::value::TAG_STR {
        if let crate::gc::ManagedObject::Str(s) = rt.heap.get(args[0].as_obj_id()) {
            s.to_string()
        } else {
            return Err("open expects text".into());
        }
    } else {
        return Err("open expects text".into());
    };
    rt.fs_metadata(&path)?;
    Ok(Value::file(rt.heap.alloc(crate::gc::ManagedObject::File(
        Box::new(FileHandle {
            path,
            open: true,
            content: "".to_string(),
        }),
    ))))
}

pub(super) fn builtin_input(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() > 1 {
        return Err("input expects 0 or 1 argument".into());
    }
    if let Some(prompt) = args.first() {
        rt.write_output(&value_to_string(prompt, &rt.heap));
    }
    let mut line = String::new();
    let mut stdin = std::io::stdin().lock();
    let _ = stdin.read_line(&mut line);
    Ok(Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(
        line.trim_end_matches(['\n', '\r']).to_string().into(),
    ))))
}

pub(super) fn builtin_time_unix(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("time_unix expects 0 arguments".into());
    }
    Ok(Value::from_i64(rt.clock_unix_secs()))
}

pub(super) fn builtin_time_millis(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("time_millis expects 0 arguments".into());
    }
    Ok(Value::from_i64(rt.clock_unix_millis()))
}

pub(super) fn builtin_mono_micros(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("mono_micros expects 0 arguments".into());
    }
    Ok(Value::from_i64(rt.clock_mono_micros()))
}

pub(super) fn builtin_mono_nanos(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("mono_nanos expects 0 arguments".into());
    }
    Ok(Value::from_i64(rt.clock_mono_nanos()))
}

pub(super) fn builtin_abs(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("abs expects 1 argument".into());
    }
    let v = &args[0];
    if v.is_int() {
        Ok(Value::from_i64(v.as_i64().abs()))
    } else if v.is_f64() {
        Ok(Value::from_f64(v.as_f64().abs()))
    } else {
        Err("abs expects number".into())
    }
}

pub(super) fn builtin_max(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("max expects 2 arguments".into());
    }
    if args[0].is_int() && args[1].is_int() {
        Ok(Value::from_i64(args[0].as_i64().max(args[1].as_i64())))
    } else {
        let (a, b) = to_f64_pair(&args[0], &args[1])?;
        Ok(Value::from_f64(a.max(b)))
    }
}

pub(super) fn builtin_min(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("min expects 2 arguments".into());
    }
    if args[0].is_int() && args[1].is_int() {
        Ok(Value::from_i64(args[0].as_i64().min(args[1].as_i64())))
    } else {
        let (a, b) = to_f64_pair(&args[0], &args[1])?;
        Ok(Value::from_f64(a.min(b)))
    }
}

pub(super) fn builtin_parse_int(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("parse_int expects 1 argument".into());
    }
    let v = &args[0];
    if v.get_tag() == crate::value::TAG_STR {
        if let crate::gc::ManagedObject::Str(s) = _rt.heap.get(v.as_obj_id()) {
            let ss = s.trim();
            if let Ok(v) = ss.parse::<i64>() {
                Ok(Value::from_i64(v))
            } else if let Ok(fv) = ss.parse::<f64>() {
                Ok(Value::from_i64(fv as i64))
            } else {
                Err("parse_int expects numeric text".into())
            }
        } else {
            Err("parse_int expects text".into())
        }
    } else if v.is_int() {
        Ok(Value::from_i64(v.as_i64()))
    } else if v.is_f64() {
        Ok(Value::from_i64(v.as_f64() as i64))
    } else {
        Err("parse_int expects text or number".into())
    }
}

pub(super) fn builtin_parse_float(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("parse_float expects 1 argument".into());
    }
    let v = &args[0];
    if v.get_tag() == crate::value::TAG_STR {
        if let crate::gc::ManagedObject::Str(s) = _rt.heap.get(v.as_obj_id()) {
            let ss = s.trim();
            let v = ss
                .parse::<f64>()
                .map_err(|_| "parse_float expects numeric text".to_string())?;
            Ok(Value::from_f64(v))
        } else {
            Err("parse_float expects text".into())
        }
    } else if v.is_int() {
        Ok(Value::from_f64(v.as_i64() as f64))
    } else if v.is_f64() {
        Ok(Value::from_f64(v.as_f64()))
    } else {
        Err("parse_float expects text or number".into())
    }
}

pub(super) fn builtin_rand(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() > 1 {
        return Err("rand expects 0 or 1 argument".into());
    }
    let raw = rt.rng_next_u64();
    if let Some(upper) = args.first() {
        let u = to_i64(upper)?;
        if u <= 0 {
            return Err("rand upper bound must be > 0".into());
        }
        Ok(Value::from_i64((raw % (u as u64)) as i64))
    } else {
        Ok(Value::from_i64(raw as i64))
    }
}

pub(super) fn builtin_to_text(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("to_text expects 1 argument".into());
    }
    let v = &args[0];
    let s = if v.get_tag() == crate::value::TAG_STR {
        if let crate::gc::ManagedObject::Str(x) = rt.heap.get(v.as_obj_id()) {
            x.clone()
        } else {
            "text".into()
        }
    } else if v.is_void() {
        "()".into()
    } else if v.is_bool() {
        if v.as_bool() {
            "true".into()
        } else {
            "false".into()
        }
    } else if v.is_int() {
        i64_to_text_fast(v.as_i64())
    } else if v.is_f64() {
        let f = v.as_f64();
        if f.fract() == 0.0 {
            i64_to_text_fast(f as i64)
        } else {
            f.to_string().into()
        }
    } else {
        value_to_string(v, &rt.heap).into()
    };
    Ok(Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(s))))
}

pub(super) fn builtin_builder_new(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("builder_new expects 0 arguments".into());
    }
    Ok(Value::builder(
        rt.heap
            .alloc(crate::gc::ManagedObject::Builder(String::new())),
    ))
}

pub(super) fn builtin_builder_new_with_capacity(
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
        crate::gc::ManagedObject::Builder(String::with_capacity(cap)),
    )))
}

pub(super) fn builtin_builder_push(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("builder_push expects 2 arguments".into());
    }
    let id = if args[0].get_tag() == crate::value::TAG_BUILDER {
        args[0].as_obj_id()
    } else {
        return Err("builder_push first arg must be builder".into());
    };

    let v = &args[1];
    if v.is_void() {
        if let crate::gc::ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
            sb.push_str("()");
        }
    } else if v.is_bool() {
        let s = if v.as_bool() { "true" } else { "false" };
        if let crate::gc::ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
            sb.push_str(s);
        }
    } else if v.is_int() {
        let mut buf = itoa::Buffer::new();
        let digits = buf.format(v.as_i64());
        if let crate::gc::ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
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
        if let crate::gc::ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
            sb.push_str(&s);
        }
    } else if v.get_tag() == crate::value::TAG_STR {
        let text = if let crate::gc::ManagedObject::Str(s) = rt.heap.get(v.as_obj_id()) {
            s.clone()
        } else {
            "".into()
        };
        if let crate::gc::ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
            sb.push_str(text.as_str());
        }
    } else if v.get_tag() == crate::value::TAG_BUILDER {
        let s = if let crate::gc::ManagedObject::Builder(s) = rt.heap.get(v.as_obj_id()) {
            s.clone()
        } else {
            String::new()
        };
        if let crate::gc::ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
            sb.push_str(&s);
        }
    } else {
        let s = value_to_string(v, &rt.heap);
        if let crate::gc::ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
            sb.push_str(&s);
        }
    }

    Ok(Value::VOID)
}

pub(super) fn builtin_builder_finalize(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("builder_finalize expects 1 argument".into());
    }
    let v = &args[0];
    if v.get_tag() == crate::value::TAG_BUILDER {
        let id = v.as_obj_id();
        if let crate::gc::ManagedObject::Builder(s) = rt.heap.get(id) {
            Ok(Value::str(
                rt.heap
                    .alloc(crate::gc::ManagedObject::Str(s.clone().into())),
            ))
        } else {
            Err("Not a builder".into())
        }
    } else {
        Err("builder_finalize expects builder".into())
    }
}

pub(super) fn builtin_os_args(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("os_args expects 0 arguments".into());
    }
    let items = rt
        .args
        .iter()
        .map(|s| {
            Value::str(
                rt.heap
                    .alloc(crate::gc::ManagedObject::Str(s.clone().into())),
            )
        })
        .collect::<Vec<_>>();
    Ok(Value::list(
        rt.heap.alloc(crate::gc::ManagedObject::List(items)),
    ))
}

pub(super) fn builtin_sin(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("sin expects 1 argument".into());
    }
    let v = to_f64(&args[0])?;
    Ok(Value::from_f64(v.sin()))
}

pub(super) fn builtin_cos(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("cos expects 1 argument".into());
    }
    let v = to_f64(&args[0])?;
    Ok(Value::from_f64(v.cos()))
}

pub(super) fn builtin_tan(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("tan expects 1 argument".into());
    }
    let v = to_f64(&args[0])?;
    Ok(Value::from_f64(v.tan()))
}

pub(super) fn builtin_sqrt(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("sqrt expects 1 argument".into());
    }
    let v = to_f64(&args[0])?;
    Ok(Value::from_f64(v.sqrt()))
}

pub(super) fn builtin_log(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("log expects 1 argument".into());
    }
    let v = to_f64(&args[0])?;
    Ok(Value::from_f64(v.ln()))
}

pub(super) fn builtin_pow(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("pow expects 2 arguments".into());
    }
    let (base, exp) = to_f64_pair(&args[0], &args[1])?;
    Ok(Value::from_f64(base.powf(exp)))
}

pub(super) fn builtin_contains(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("contains expects 2 arguments".into());
    }
    let hay = &args[0];
    let needle = &args[1];
    if hay.get_tag() != crate::value::TAG_STR || needle.get_tag() != crate::value::TAG_STR {
        return Err("contains expects (text, text)".into());
    }
    let hay_id = hay.as_obj_id();
    let needle_id = needle.as_obj_id();

    // Get references without cloning
    let result = if let (
        crate::gc::ManagedObject::Str(hs),
        crate::gc::ManagedObject::Str(ns),
    ) = (rt.heap.get(hay_id), rt.heap.get(needle_id)) {
        hs.as_str().contains(ns.as_str())
    } else {
        return Err("contains expects text".into());
    };
    Ok(Value::from_bool(result))
}

pub(super) fn builtin_starts_with(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("starts_with expects 2 arguments".into());
    }
    let hay = &args[0];
    let prefix = &args[1];
    if hay.get_tag() != crate::value::TAG_STR || prefix.get_tag() != crate::value::TAG_STR {
        return Err("starts_with expects (text, text)".into());
    }
    let hay_id = hay.as_obj_id();
    let prefix_id = prefix.as_obj_id();

    // Get references without cloning
    let result = if let (
        crate::gc::ManagedObject::Str(hs),
        crate::gc::ManagedObject::Str(ps),
    ) = (rt.heap.get(hay_id), rt.heap.get(prefix_id)) {
        hs.as_str().starts_with(ps.as_str())
    } else {
        return Err("starts_with expects text".into());
    };
    Ok(Value::from_bool(result))
}

pub(super) fn builtin_ends_with(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("ends_with expects 2 arguments".into());
    }
    let hay = &args[0];
    let suffix = &args[1];
    if hay.get_tag() != crate::value::TAG_STR || suffix.get_tag() != crate::value::TAG_STR {
        return Err("ends_with expects (text, text)".into());
    }
    let hay_id = hay.as_obj_id();
    let suffix_id = suffix.as_obj_id();

    // Get references without cloning
    let result = if let (
        crate::gc::ManagedObject::Str(hs),
        crate::gc::ManagedObject::Str(ss),
    ) = (rt.heap.get(hay_id), rt.heap.get(suffix_id)) {
        hs.as_str().ends_with(ss.as_str())
    } else {
        return Err("ends_with expects text".into());
    };
    Ok(Value::from_bool(result))
}

pub(super) fn builtin_process_rss(_rt: &mut Runtime, _args: &[Value]) -> Result<Value, String> {
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

pub(super) fn builtin_assert(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() < 1 || args.len() > 2 {
        return Err("__builtin_assert expects 1 or 2 arguments".into());
    }
    let cond = &args[0];
    if !cond.is_bool() {
        return Err("__builtin_assert expects bool".into());
    }
    if !cond.as_bool() {
        let msg = if args.len() == 2 {
            value_to_string(&args[1], &rt.heap)
        } else {
            "Assertion failed".to_string()
        };
        return Err(msg);
    }
    Ok(Value::VOID)
}

pub(super) fn builtin_assert_eq(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err("__builtin_assert_eq expects 2 or 3 arguments".into());
    }
    let a = &args[0];
    let b = &args[1];
    if !rt.values_equal(a, b) {
        let msg = if args.len() == 3 {
            value_to_string(&args[2], &rt.heap)
        } else {
            let sa = value_to_string(a, &rt.heap);
            let sb = value_to_string(b, &rt.heap);
            format!("Assertion failed: {} != {}", sa, sb)
        };
        return Err(msg);
    }
    Ok(Value::VOID)
}

fn to_f64(v: &Value) -> Result<f64, String> {
    if v.is_int() {
        Ok(v.as_i64() as f64)
    } else if v.is_f64() {
        Ok(v.as_f64())
    } else {
        Err(format!("Expected number, got {}", v.type_name()))
    }
}

/// Create a set (dict with unit values) from a list
pub(super) fn builtin_set_from_list(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("__set_from_list expects 1 argument".into());
    }
    let list = &args[0];
    if list.get_tag() != crate::value::TAG_LIST {
        return Err("__set_from_list expects list".into());
    }
    let items = if let crate::gc::ManagedObject::List(items) = rt.heap.get(list.as_obj_id()) {
        items.clone()
    } else {
        return Err("__set_from_list expects list".into());
    };

    let mut dict = crate::value::dict_with_capacity(items.len());
    for item in items {
        let key = if item.get_tag() == crate::value::TAG_STR {
            if let crate::gc::ManagedObject::Str(s) = rt.heap.get(item.as_obj_id()) {
                crate::value::DictKey::from_text(s)
            } else {
                return Err("Invalid set item".into());
            }
        } else if item.is_int() {
            crate::value::DictKey::Int(item.as_i64())
        } else {
            return Err("Set items must be int or string".into());
        };
        dict.map.insert(key, Value::VOID);
    }

    Ok(Value::dict(rt.heap.alloc(crate::gc::ManagedObject::Dict(dict))))
}

pub(super) fn builtin_heap_stats(rt: &mut Runtime, _args: &[Value]) -> Result<Value, String> {
    let stats = rt.heap.memory_stats();
    rt.write_output(&stats);
    rt.write_output("\n");
    // Print type sizes
    rt.write_output(&format!(
        "Type sizes: Text={}, DictKey={}, Value={}, ManagedObject={}\n",
        std::mem::size_of::<crate::Text>(),
        std::mem::size_of::<crate::value::DictKey>(),
        std::mem::size_of::<Value>(),
        std::mem::size_of::<crate::gc::ManagedObject>()
    ));
    rt.write_output(&format!(
        "Variant sizes: List={}, Dict={}, DictStr={}, Str={}, Function={}, Module={}, Shape={}\n",
        std::mem::size_of::<Vec<Value>>(),
        std::mem::size_of::<crate::value::Dict>(),
        std::mem::size_of::<crate::value::DictStr>(),
        std::mem::size_of::<crate::Text>(),
        std::mem::size_of::<crate::value::Function>(),
        std::mem::size_of::<crate::value::ModuleInstance>(),
        std::mem::size_of::<crate::value::Shape>()
    ));
    Ok(Value::VOID)
}
