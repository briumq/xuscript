use smallvec::SmallVec;
use xu_ir::Expr;
use crate::Value;

use crate::Runtime;

impl Runtime {
    pub(crate) fn eval_args(&mut self, args: &[Expr]) -> Result<SmallVec<[Value; 4]>, String> {
        let mut out: SmallVec<[Value; 4]> = SmallVec::with_capacity(args.len());
        let roots_base = self.gc_temp_roots.len();
        for a in args {
            let v = self.eval_expr(a)?;
            // Push to gc_temp_roots as GC root protection
            self.gc_temp_roots.push(v);
            out.push(v);
        }
        // Pop the temporary roots
        self.gc_temp_roots.truncate(roots_base);
        Ok(out)
    }
}
