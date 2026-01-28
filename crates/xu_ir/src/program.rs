use crate::{Bytecode, Module};

#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub module: Module,
    pub bytecode: Option<Bytecode>,
}
