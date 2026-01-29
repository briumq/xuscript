use smallvec::SmallVec;
use xu_ir::Expr;
use crate::Value;

use super::Runtime;

impl Runtime {
    pub(super) fn eval_args(&mut self, args: &[Expr]) -> Result<SmallVec<[Value; 4]>, String> {
        let mut out: SmallVec<[Value; 4]> = SmallVec::with_capacity(args.len());
        for a in args {
            out.push(self.eval_expr(a)?);
        }
        Ok(out)
    }
}
