use xu_ir::{Expr, Stmt};
use crate::Value;

use super::{Flow, Runtime};

impl Runtime {
    pub(super) fn exec_if_branches(
        &mut self,
        branches: &[(Expr, Box<[Stmt]>)],
        else_branch: Option<&Box<[Stmt]>>,
    ) -> Flow {
        for (cond, body) in branches {
            match self.eval_expr(cond) {
                Ok(v) if v.is_bool() && v.as_bool() => return self.exec_stmts(body.as_ref()),
                Ok(v) if v.is_bool() && !v.as_bool() => continue,
                Ok(v) => {
                    let err_msg =
                        self.error(xu_syntax::DiagnosticKind::InvalidConditionType(
                            v.type_name().to_string(),
                        ));
                    let err_val = Value::str(
                        self.heap
                            .alloc(crate::gc::ManagedObject::Str(err_msg.into())),
                    );
                    return Flow::Throw(err_val);
                }
                Err(e) => {
                    let err_val = Value::str(
                        self.heap.alloc(crate::gc::ManagedObject::Str(e.into())),
                    );
                    return Flow::Throw(err_val);
                }
            }
        }
        if let Some(body) = else_branch {
            self.exec_stmts(body.as_ref())
        } else {
            Flow::None
        }
    }

    pub(super) fn exec_while_loop(&mut self, cond: &Expr, body: &Box<[Stmt]>) -> Flow {
        loop {
            let cond_v = match self.eval_expr(cond) {
                Ok(v) if v.is_bool() => v.as_bool(),
                Ok(v) => {
                    let err_msg =
                        self.error(xu_syntax::DiagnosticKind::InvalidConditionType(
                            v.type_name().to_string(),
                        ));
                    let err_val = Value::str(
                        self.heap
                            .alloc(crate::gc::ManagedObject::Str(err_msg.into())),
                    );
                    return Flow::Throw(err_val);
                }
                Err(e) => {
                    let err_val =
                        Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(e.into())));
                    return Flow::Throw(err_val);
                }
            };
            if !cond_v {
                break;
            }
            match self.exec_stmts(body.as_ref()) {
                Flow::None => {}
                Flow::Continue => continue,
                Flow::Break => break,
                other => return other,
            }
        }
        Flow::None
    }
}
