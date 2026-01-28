use xu_ir::Constant;
use xu_runtime::{Bytecode, Op, VM};

fn add_const_str(bc: &mut Bytecode, s: &str) -> u32 {
    let idx = bc.constants.len() as u32;
    bc.constants.push(Constant::Str(s.to_string()));
    idx
}

#[test]
fn vm_ir_print_hello() {
    let mut bc = Bytecode::default();
    let s = add_const_str(&mut bc, "Hello");
    bc.ops.push(Op::Const(s));
    bc.ops.push(Op::Print);
    bc.ops.push(Op::Halt);
    let mut vm = VM::new();
    vm.run(&bc).unwrap();
    assert_eq!(vm.output.trim_end(), "Hello");
}

#[test]
fn vm_ir_add_numbers() {
    let mut bc = Bytecode::default();
    bc.ops.push(Op::ConstInt(2));
    bc.ops.push(Op::ConstInt(3));
    bc.ops.push(Op::Add);
    bc.ops.push(Op::Print);
    bc.ops.push(Op::Halt);
    let mut vm = VM::new();
    vm.run(&bc).unwrap();
    assert_eq!(vm.output.trim_end(), "5");
}

#[test]
fn vm_ir_add_string_number() {
    let mut bc = Bytecode::default();
    let s = add_const_str(&mut bc, "X");
    bc.ops.push(Op::Const(s));
    bc.ops.push(Op::ConstInt(1));
    bc.ops.push(Op::Add);
    bc.ops.push(Op::Print);
    bc.ops.push(Op::Halt);
    let mut vm = VM::new();
    vm.run(&bc).unwrap();
    assert_eq!(vm.output.trim_end(), "X1");
}

#[test]
fn vm_ir_while_loop_prints_0_1_2() {
    let mut bc = Bytecode::default();
    // i = 0
    bc.ops.push(Op::ConstInt(0));
    let i_name = add_const_str(&mut bc, "i");
    bc.ops.push(Op::StoreName(i_name));
    // loop start:
    let loop_start = bc.ops.len();
    // i < 3
    bc.ops.push(Op::LoadName(i_name));
    bc.ops.push(Op::ConstInt(3));
    bc.ops.push(Op::Lt);
    // placeholder JumpIfFalse, will be patched by VM run loop via positions
    let jfalse_pos = bc.ops.len();
    bc.ops.push(Op::JumpIfFalse(usize::MAX));
    // print i
    bc.ops.push(Op::LoadName(i_name));
    bc.ops.push(Op::Print);
    // i = i + 1
    bc.ops.push(Op::LoadName(i_name));
    bc.ops.push(Op::ConstInt(1));
    bc.ops.push(Op::Add);
    bc.ops.push(Op::StoreName(i_name));
    // jump to loop_start
    bc.ops.push(Op::Jump(loop_start));
    // end label
    let end = bc.ops.len();
    if let Op::JumpIfFalse(to) = &mut bc.ops[jfalse_pos] {
        *to = end;
    }
    bc.ops.push(Op::Halt);

    let mut vm = VM::new();
    vm.run(&bc).unwrap();
    assert_eq!(vm.output.trim_end(), "0\n1\n2");
}
