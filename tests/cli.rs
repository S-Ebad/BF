//! Command-line behavior.

mod common;

use common::*;

const PROGRAM: &str = include_str!("bf/print_f.bf");

// Emit modes and output paths

#[test]
fn default_emit_is_executable() {
    let ws = Workspace::new();
    ws.write("f.bf", PROGRAM);

    assert!(ws.brainfk(["f.bf"]).status.success());
    assert_eq!(run(&ws.path("a.out"), b"").stdout, b"F");
}

#[test]
fn emit_asm() {
    let ws = Workspace::new();
    ws.write("f.bf", PROGRAM);

    assert!(ws.brainfk(["f.bf", "--emit", "asm"]).status.success());
    let asm = std::fs::read_to_string(ws.path("a.asm")).unwrap();
    assert!(asm.contains("global _start"));
}

#[test]
fn emit_obj() {
    let ws = Workspace::new();
    ws.write("f.bf", PROGRAM);

    assert!(ws.brainfk(["f.bf", "--emit", "obj"]).status.success());
    let obj = std::fs::read(ws.path("a.o")).unwrap();
    assert_eq!(&obj[..4], b"\x7fELF");
    assert_eq!(obj[16], 1, "expected a relocatable object (ET_REL)");
}

#[test]
fn output_flag() {
    let ws = Workspace::new();
    ws.write("f.bf", PROGRAM);

    for (emit, name) in [("asm", "x.s"), ("obj", "x.obj"), ("exe", "x")] {
        assert!(
            ws.brainfk(["f.bf", "--emit", emit, "-o", name])
                .status
                .success()
        );
        assert!(ws.path(name).exists(), "--emit {emit} -o {name}");
    }
    assert!(!ws.path("a.out").exists());
    assert_eq!(run(&ws.path("x"), b"").stdout, b"F");
}

// Bad arguments and I/O failures

#[test]
fn missing_input_file() {
    let ws = Workspace::new();
    let out = ws.brainfk(["nope.bf"]);

    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn input_is_directory() {
    let ws = Workspace::new();
    std::fs::create_dir(ws.path("d")).unwrap();
    let out = ws.brainfk(["d"]);

    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn unwritable_output() {
    let ws = Workspace::new();
    ws.write("f.bf", PROGRAM);

    let out = ws.brainfk(["f.bf", "--emit", "asm", "-o", "missing/dir/out.asm"]);
    assert_eq!(out.status.code(), Some(1));

    let out = ws.brainfk(["f.bf", "-o", "missing/dir/out"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn rejects_bad_arguments() {
    let ws = Workspace::new();
    ws.write("f.bf", PROGRAM);

    for args in [&["f.bf", "-O3"][..], &["f.bf", "--emit", "wat"], &[]] {
        let out = ws.brainfk(args);
        assert_eq!(out.status.code(), Some(2), "args: {args:?}");
    }
}

// Compile errors (the errors themselves are unit-tested in src/)

#[test]
fn compile_error_fails_without_output() {
    let ws = Workspace::new();
    ws.write("e.bf", "[");

    for emit in ["asm", "obj", "exe"] {
        let out = ws.brainfk(["e.bf", "--emit", emit, "-o", "out"]);
        assert_eq!(out.status.code(), Some(1), "--emit {emit}");
        assert!(!ws.path("out").exists(), "--emit {emit} wrote output");
    }
}
