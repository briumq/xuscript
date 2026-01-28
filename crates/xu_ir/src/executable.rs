use crate::{Module, Program};

#[derive(Clone, Debug, PartialEq)]
pub enum Executable {
    Ast(Module),
    Bytecode(Program),
}
