// Converted from the tests for uBPF <https://github.com/iovisor/ubpf>
// Copyright 2015 Big Switch Networks, Inc
// Copyright 2016 6WIND S.A. <quentin.monnet@6wind.com>
//
// Licensed under the Apache License, Version 2.0 <http://www.apache.org/licenses/LICENSE-2.0> or
// the MIT license <http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

// The tests contained in this file are extracted from the unit tests of uBPF software. Each test
// in this file has a name in the form `test_verifier_<name>`, and corresponds to the
// (human-readable) code in `ubpf/tree/master/tests/<name>`, available at
// <https://github.com/iovisor/ubpf/tree/master/tests> (hyphen had to be replaced with underscores
// as Rust will not accept them in function names). It is strongly advised to refer to the uBPF
// version to understand what these program do.
//
// Each program was assembled from the uBPF version with the assembler provided by uBPF itself, and
// available at <https://github.com/iovisor/ubpf/tree/master/ubpf>.
// The very few modifications that have been realized should be indicated.

// These are unit tests for the eBPF “verifier”.

extern crate solana_rbpf;
extern crate thiserror;

use solana_rbpf::{
    assembler::assemble,
    ebpf,
    elf::Executable,
    verifier::{RequisiteVerifier, TautologyVerifier, Verifier, VerifierError},
    vm::{BuiltInProgram, Config, FunctionRegistry, TestContextObject},
};
use std::sync::Arc;
use test_utils::create_vm;
use thiserror::Error;

/// Error definitions
#[derive(Debug, Error)]
pub enum VerifierTestError {
    #[error("{0}")]
    Rejected(String),
}

struct ContradictionVerifier {}
impl Verifier for ContradictionVerifier {
    fn verify(
        _prog: &[u8],
        _config: &Config,
        _function_registry: &FunctionRegistry,
    ) -> std::result::Result<(), VerifierError> {
        Err(VerifierError::NoProgram)
    }
}

#[test]
fn test_verifier_success() {
    let executable = assemble::<TestContextObject>(
        "
        mov32 r0, 0xBEE
        exit",
        Arc::new(BuiltInProgram::new_loader(Config::default())),
    )
    .unwrap();
    let verified_executable =
        Executable::<TautologyVerifier, TestContextObject>::verified(executable).unwrap();
    create_vm!(
        _vm,
        &verified_executable,
        &mut TestContextObject::default(),
        stack,
        heap,
        Vec::new(),
        None
    );
}

#[test]
#[should_panic(expected = "NoProgram")]
fn test_verifier_fail() {
    let executable = assemble::<TestContextObject>(
        "
        mov32 r0, 0xBEE
        exit",
        Arc::new(BuiltInProgram::new_loader(Config::default())),
    )
    .unwrap();
    let _verified_executable =
        Executable::<ContradictionVerifier, TestContextObject>::verified(executable).unwrap();
}

#[test]
#[should_panic(expected = "DivisionByZero(30)")]
fn test_verifier_err_div_by_zero_imm() {
    let executable = assemble::<TestContextObject>(
        "
        mov32 r0, 1
        div32 r0, 0
        exit",
        Arc::new(BuiltInProgram::new_loader(Config::default())),
    )
    .unwrap();
    let _verified_executable =
        Executable::<RequisiteVerifier, TestContextObject>::verified(executable).unwrap();
}

#[test]
#[should_panic(expected = "UnsupportedLEBEArgument(29)")]
fn test_verifier_err_endian_size() {
    let prog = &[
        0xdc, 0x01, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, //
        0xb7, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
        0x95, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    ];
    let executable = Executable::<TautologyVerifier, TestContextObject>::from_text_bytes(
        prog,
        Arc::new(BuiltInProgram::new_loader(Config::default())),
        FunctionRegistry::default(),
    )
    .unwrap();
    let _verified_executable =
        Executable::<RequisiteVerifier, TestContextObject>::verified(executable).unwrap();
}

#[test]
#[should_panic(expected = "IncompleteLDDW(29)")]
fn test_verifier_err_incomplete_lddw() {
    // Note: ubpf has test-err-incomplete-lddw2, which is the same
    let prog = &[
        0x18, 0x00, 0x00, 0x00, 0x88, 0x77, 0x66, 0x55, //
        0x95, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    ];
    let executable = Executable::<TautologyVerifier, TestContextObject>::from_text_bytes(
        prog,
        Arc::new(BuiltInProgram::new_loader(Config::default())),
        FunctionRegistry::default(),
    )
    .unwrap();
    let _verified_executable =
        Executable::<RequisiteVerifier, TestContextObject>::verified(executable).unwrap();
}

#[test]
fn test_verifier_err_invalid_reg_dst() {
    // r11 is disabled when dynamic_stack_frames=false, and only sub and add are
    // allowed when dynamic_stack_frames=true
    for dynamic_stack_frames in [false, true] {
        let executable = assemble::<TestContextObject>(
            "
            mov r11, 1
            exit",
            Arc::new(BuiltInProgram::new_loader(Config {
                dynamic_stack_frames,
                ..Config::default()
            })),
        )
        .unwrap();
        let result = Executable::<RequisiteVerifier, TestContextObject>::verified(executable)
            .map_err(|err| format!("Executable constructor {err:?}"));

        assert_eq!(
            result.unwrap_err(),
            "Executable constructor VerifierError(InvalidDestinationRegister(29))"
        );
    }
}

#[test]
fn test_verifier_err_invalid_reg_src() {
    // r11 is disabled when dynamic_stack_frames=false, and only sub and add are
    // allowed when dynamic_stack_frames=true
    for dynamic_stack_frames in [false, true] {
        let executable = assemble::<TestContextObject>(
            "
            mov r0, r11
            exit",
            Arc::new(BuiltInProgram::new_loader(Config {
                dynamic_stack_frames,
                ..Config::default()
            })),
        )
        .unwrap();
        let result = Executable::<RequisiteVerifier, TestContextObject>::verified(executable)
            .map_err(|err| format!("Executable constructor {err:?}"));

        assert_eq!(
            result.unwrap_err(),
            "Executable constructor VerifierError(InvalidSourceRegister(29))"
        );
    }
}

#[test]
fn test_verifier_resize_stack_ptr_success() {
    let executable = assemble::<TestContextObject>(
        "
        sub r11, 1
        add r11, 1
        exit",
        Arc::new(BuiltInProgram::new_loader(Config {
            enable_stack_frame_gaps: false,
            ..Config::default()
        })),
    )
    .unwrap();
    let _verified_executable =
        Executable::<RequisiteVerifier, TestContextObject>::verified(executable).unwrap();
}

#[test]
#[should_panic(expected = "JumpToMiddleOfLDDW(2, 29)")]
fn test_verifier_err_jmp_lddw() {
    let executable = assemble::<TestContextObject>(
        "
        ja +1
        lddw r0, 0x1122334455667788
        exit",
        Arc::new(BuiltInProgram::new_loader(Config::default())),
    )
    .unwrap();
    let _verified_executable =
        Executable::<RequisiteVerifier, TestContextObject>::verified(executable).unwrap();
}

#[test]
#[should_panic(expected = "InvalidFunction(1)")]
fn test_verifier_err_call_lddw() {
    let executable = assemble::<TestContextObject>(
        "
        call 1
        lddw r0, 0x1122334455667788
        exit",
        Arc::new(BuiltInProgram::new_loader(Config::default())),
    )
    .unwrap();
    let _verified_executable =
        Executable::<RequisiteVerifier, TestContextObject>::verified(executable).unwrap();
}

#[test]
#[should_panic(expected = "InvalidFunction(0)")]
fn test_verifier_err_function_fallthrough() {
    let executable = assemble::<TestContextObject>(
        "
        mov r0, r1
        function_foo:
        exit",
        Arc::new(BuiltInProgram::new_loader(Config::default())),
    )
    .unwrap();
    let _verified_executable =
        Executable::<RequisiteVerifier, TestContextObject>::verified(executable).unwrap();
}

#[test]
#[should_panic(expected = "JumpOutOfCode(3, 29)")]
fn test_verifier_err_jmp_out() {
    let executable = assemble::<TestContextObject>(
        "
        ja +2
        exit",
        Arc::new(BuiltInProgram::new_loader(Config::default())),
    )
    .unwrap();
    let _verified_executable =
        Executable::<RequisiteVerifier, TestContextObject>::verified(executable).unwrap();
}

#[test]
#[should_panic(expected = "JumpOutOfCode(18446744073709551615, 29)")]
fn test_verifier_err_jmp_out_start() {
    let executable = assemble::<TestContextObject>(
        "
        ja -2
        exit",
        Arc::new(BuiltInProgram::new_loader(Config::default())),
    )
    .unwrap();
    let _verified_executable =
        Executable::<RequisiteVerifier, TestContextObject>::verified(executable).unwrap();
}

#[test]
#[should_panic(expected = "UnknownOpCode(6, 29)")]
fn test_verifier_err_unknown_opcode() {
    let prog = &[
        0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
        0x95, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    ];
    let executable = Executable::<TautologyVerifier, TestContextObject>::from_text_bytes(
        prog,
        Arc::new(BuiltInProgram::new_loader(Config::default())),
        FunctionRegistry::default(),
    )
    .unwrap();
    let _verified_executable =
        Executable::<RequisiteVerifier, TestContextObject>::verified(executable).unwrap();
}

#[test]
#[should_panic(expected = "CannotWriteR10(29)")]
fn test_verifier_err_write_r10() {
    let executable = assemble::<TestContextObject>(
        "
        mov r10, 1
        exit",
        Arc::new(BuiltInProgram::new_loader(Config::default())),
    )
    .unwrap();
    let _verified_executable =
        Executable::<RequisiteVerifier, TestContextObject>::verified(executable).unwrap();
}

#[test]
fn test_verifier_err_all_shift_overflows() {
    let testcases = [
        // lsh32_imm
        ("lsh32 r0, 16", Ok(())),
        ("lsh32 r0, 32", Err("ShiftWithOverflow(32, 32, 29)")),
        ("lsh32 r0, 64", Err("ShiftWithOverflow(64, 32, 29)")),
        // rsh32_imm
        ("rsh32 r0, 16", Ok(())),
        ("rsh32 r0, 32", Err("ShiftWithOverflow(32, 32, 29)")),
        ("rsh32 r0, 64", Err("ShiftWithOverflow(64, 32, 29)")),
        // arsh32_imm
        ("arsh32 r0, 16", Ok(())),
        ("arsh32 r0, 32", Err("ShiftWithOverflow(32, 32, 29)")),
        ("arsh32 r0, 64", Err("ShiftWithOverflow(64, 32, 29)")),
        // lsh64_imm
        ("lsh64 r0, 32", Ok(())),
        ("lsh64 r0, 64", Err("ShiftWithOverflow(64, 64, 29)")),
        // rsh64_imm
        ("rsh64 r0, 32", Ok(())),
        ("rsh64 r0, 64", Err("ShiftWithOverflow(64, 64, 29)")),
        // arsh64_imm
        ("arsh64 r0, 32", Ok(())),
        ("arsh64 r0, 64", Err("ShiftWithOverflow(64, 64, 29)")),
    ];

    for (overflowing_instruction, expected) in testcases {
        let assembly = format!("\n{overflowing_instruction}\nexit");
        let executable = assemble::<TestContextObject>(
            &assembly,
            Arc::new(BuiltInProgram::new_loader(Config::default())),
        )
        .unwrap();
        let result = Executable::<RequisiteVerifier, TestContextObject>::verified(executable)
            .map_err(|err| format!("Executable constructor {err:?}"));
        match expected {
            Ok(()) => assert!(result.is_ok()),
            Err(overflow_msg) => match result {
                Err(err) => assert_eq!(
                    err,
                    format!("Executable constructor VerifierError({overflow_msg})"),
                ),
                _ => panic!("Expected error"),
            },
        }
    }
}

#[test]
fn test_sdiv_disabled() {
    let instructions = [
        (ebpf::SDIV32_IMM, "sdiv32 r0, 2"),
        (ebpf::SDIV32_REG, "sdiv32 r0, r1"),
        (ebpf::SDIV64_IMM, "sdiv64 r0, 4"),
        (ebpf::SDIV64_REG, "sdiv64 r0, r1"),
    ];

    for (opc, instruction) in instructions {
        for enable_sdiv in [true, false] {
            let assembly = format!("\n{instruction}\nexit");
            let executable = assemble::<TestContextObject>(
                &assembly,
                Arc::new(BuiltInProgram::new_loader(Config {
                    enable_sdiv,
                    ..Config::default()
                })),
            )
            .unwrap();
            let result = Executable::<RequisiteVerifier, TestContextObject>::verified(executable)
                .map_err(|err| format!("Executable constructor {err:?}"));
            if enable_sdiv {
                assert!(result.is_ok());
            } else {
                assert_eq!(
                    result.unwrap_err(),
                    format!(
                        "Executable constructor VerifierError(UnknownOpCode({}, {}))",
                        opc,
                        ebpf::ELF_INSN_DUMP_OFFSET
                    ),
                );
            }
        }
    }
}
