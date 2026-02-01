use std::io::BufRead;

use super::super::Runtime;
use super::super::util::value_to_string;
use crate::Value;

pub fn builtin_print(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    for a in args {
        rt.write_output(&value_to_string(a, &rt.heap));
    }
    Ok(Value::VOID)
}

pub fn builtin_gen_id(rt: &mut Runtime, _args: &[Value]) -> Result<Value, String> {
    let id = rt.next_id;
    rt.next_id = rt.next_id.saturating_add(1);
    Ok(Value::from_i64(id as i64))
}

pub fn builtin_gc(rt: &mut Runtime, _args: &[Value]) -> Result<Value, String> {
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

pub fn builtin_open(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("open expects 1 argument".into());
    }
    let path = if args[0].get_tag() == crate::core::value::TAG_STR {
        if let crate::core::heap::ManagedObject::Str(s) = rt.heap.get(args[0].as_obj_id()) {
            s.to_string()
        } else {
            return Err("open expects text".into());
        }
    } else {
        return Err("open expects text".into());
    };
    rt.fs_metadata(&path)?;
    Ok(Value::file(rt.heap.alloc(crate::core::heap::ManagedObject::File(
        Box::new(crate::core::value::FileHandle {
            path,
            open: true,
            content: "".to_string(),
        }),
    ))))
}

pub fn builtin_input(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() > 1 {
        return Err("input expects 0 or 1 argument".into());
    }
    if let Some(prompt) = args.first() {
        rt.write_output(&value_to_string(prompt, &rt.heap));
    }
    let mut line = String::new();
    let mut stdin = std::io::stdin().lock();
    let _ = stdin.read_line(&mut line);
    Ok(Value::str(rt.heap.alloc(crate::core::heap::ManagedObject::Str(
        line.trim_end_matches(['\n', '\r']).to_string().into(),
    ))))
}
