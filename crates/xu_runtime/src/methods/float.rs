use super::common::*;
use super::Runtime;
use crate::Value;

pub(crate) fn dispatch(rt: &mut Runtime, recv: Value, kind: super::MethodKind, args: &[Value], method: &str) -> Result<Value, String> {
    let f = recv.as_f64();

    match kind {
        super::MethodKind::ToString => {
            let s = f.to_string();
            Ok(create_str_value(rt, &s))
        }
        super::MethodKind::Abs => {
            let abs = f.abs();
            Ok(Value::from_f64(abs))
        }
        super::MethodKind::StrToInt => {
            let i = f as i64;
            Ok(Value::from_i64(i))
        }
        super::MethodKind::FloatRound => {
            validate_arity(rt, method, args.len(), 0, 1)?;
            
            let rounded = if args.len() == 1 {
                let digits = args[0].as_i64();
                let factor = 10.0_f64.powi(digits as i32);
                (f * factor).round() / factor
            } else {
                f.round()
            };
            Ok(Value::from_f64(rounded))
        }
        super::MethodKind::FloatFloor => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let floor = f.floor();
            Ok(Value::from_f64(floor))
        }
        super::MethodKind::FloatCeil => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let ceil = f.ceil();
            Ok(Value::from_f64(ceil))
        }
        _ => Err(err(
            rt,
            xu_syntax::DiagnosticKind::UnsupportedMethod {
                method: method.to_string(),
                ty: "float".to_string(),
            },
        )),
    }
}
