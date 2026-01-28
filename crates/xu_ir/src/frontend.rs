use xu_syntax::Diagnostic;

use crate::Executable;

#[derive(Clone, Debug)]
pub struct CompiledUnit {
    pub text: String,
    pub executable: Executable,
    pub diagnostics: Vec<Diagnostic>,
}

pub trait Frontend: Send + Sync {
    fn compile_text_no_analyze(&self, path: &str, input: &str) -> Result<CompiledUnit, String>;
}
