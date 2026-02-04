use super::common::*;
use super::Runtime;
use crate::Value;

pub(crate) fn dispatch(rt: &mut Runtime, recv: Value, kind: super::MethodKind, args: &[Value], method: &str) -> Result<Value, String> {
    let i = recv.as_i64();

    match kind {
        super::MethodKind::ToString => {
            let s = i.to_string();
            Ok(create_str_value(rt, &s))
        }
        super::MethodKind::Abs => {
            let abs = i.abs();
            Ok(Value::from_i64(abs))
        }
        super::MethodKind::IntToBase => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            
            let base = args[0].as_i64();
            if base < 2 || base > 36 {
                return Err(err(
                    rt,
                    xu_syntax::DiagnosticKind::Raw("Base must be between 2 and 36".into()),
                ));
            }
            
            let s = if i == 0 {
                "0".to_string()
            } else {
                let mut result = String::new();
                let mut n = i.abs();
                let digits = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
                while n > 0 {
                    let digit = (n % base) as usize;
                    result.push(digits[digit] as char);
                    n /= base;
                }
                if i < 0 {
                    result.push('-');
                }
                result.chars().rev().collect()
            };
            Ok(create_str_value(rt, &s))
        }
        super::MethodKind::IntIsEven => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            Ok(Value::from_bool(i % 2 == 0))
        }
        super::MethodKind::IntIsOdd => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            Ok(Value::from_bool(i % 2 != 0))
        }
        _ => Err(err(
            rt,
            xu_syntax::DiagnosticKind::UnsupportedMethod {
                method: method.to_string(),
                ty: "int".to_string(),
            },
        )),
    }
}
