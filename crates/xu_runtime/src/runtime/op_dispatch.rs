#![allow(dead_code)]

use xu_ir::Op;

pub enum OpHandler {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Unknown,
}

pub fn handler_for(op: Op) -> OpHandler {
    match op {
        Op::Add => OpHandler::Add,
        Op::Sub => OpHandler::Sub,
        Op::Mul => OpHandler::Mul,
        Op::Div => OpHandler::Div,
        Op::Mod => OpHandler::Mod,
        Op::Eq => OpHandler::Eq,
        Op::Ne => OpHandler::Ne,
        Op::Lt => OpHandler::Lt,
        Op::Le => OpHandler::Le,
        Op::Gt => OpHandler::Gt,
        Op::Ge => OpHandler::Ge,
        Op::And => OpHandler::And,
        Op::Or => OpHandler::Or,
        _ => OpHandler::Unknown,
    }
}
