# What is the plan for implementing equality, branches, label management, and backpatching?

## What did verification show?

Running `cargo run -p verification` showed that equality is not yet lowered into JVM branch bytecode.
Primitive and object equality methods currently evaluate operands and return immediately without emitting comparison instructions.
The verification run also exposed a separate method call issue: `String.equals` is emitted with a `void` descriptor instead of a `boolean` return type.

## What is the current status?

The bytecode generator now has label-based branch assembly, conditional lowering for equality, boolean materialization, and `Stmt::If` emission.
`cargo run -p verification` now passes for the current equality sources, and `./scripts/check-code.sh` is green.
The remaining work is centered on loops plus control-flow stack support for `break`, `continue`, and labels.

## What needs to be built first?

A real control-flow assembly layer should be added before implementing more branching features.
The current branch patching logic uses instruction indices as branch targets, but JVM branch instructions use bytecode offsets.
That makes the existing approach too fragile to extend for equality, `if`, loops, and labeled control flow.

## What is the implementation plan?

1. Introduce a label abstraction such as `LabelId` in `crates/bytecode/src/bytecode.rs`.
2. Add an internal emitted-instruction representation that can store unresolved branches to labels.
3. Add an assembly pass that computes byte offsets, resolves label positions, and patches branch operands.
4. Replace the current ad hoc `patch_branch` flow used by `&&` and `||` with the new label-based system.
5. Split expression lowering into two forms: one that produces a value and one that emits conditional control flow.
6. Implement `==` and `!=` lowering in conditional form for primitive and reference types.
7. Materialize boolean results with `iconst_1`, `iconst_0`, and `goto` when a comparison is used as an expression value.
8. Implement `Stmt::If` lowering on top of the new conditional emission path.
9. Implement `Stmt::While`, `Stmt::DoWhile`, and `Stmt::For` using the same label and branch infrastructure.
10. Add a control-flow stack for `break`, `continue`, and labeled statements so loop and switch support has a stable foundation.
11. Fix method call descriptor generation so `emit_method_call` uses the resolved return type instead of always producing `(... )V`.
12. Expand verification coverage for equality inside expressions and inside real control-flow statements.

## How should equality be lowered?

Primitive equality should use JVM comparison instructions that match the operand kind.

- Integer-like primitives should use `if_icmpeq` and `if_icmpne`.
- `long` should use `lcmp` followed by `ifeq` or `ifne`.
- `float` should use `fcmpl` or `fcmpg` followed by `ifeq` or `ifne`.
- `double` should use `dcmpl` or `dcmpg` followed by `ifeq` or `ifne`.
- Reference equality should use `if_acmpeq` and `if_acmpne`.
- Comparisons with `null` should use `ifnull` and `ifnonnull` when possible.

## Why should method calls be included in this plan?

The new object equality example uses `String.equals`, and verification showed that method invocation descriptors are still incomplete.
Even after branch support is added, `String.equals` will remain wrong until method call return descriptors are emitted correctly.

## What is the recommended implementation order?

1. Add label and backpatch infrastructure.
2. Migrate existing `&&` and `||` lowering to the new infrastructure.
3. Implement conditional emission for boolean expressions.
4. Implement `==` and `!=` lowering.
5. Implement `Stmt::If`.
6. Implement loops.
7. Fix method call descriptors and return typing.
8. Add more verification cases and run the verification suite.

## What checklist tracks the work?

- [x] Add `LabelId` and label binding support in `crates/bytecode/src/bytecode.rs`.
- [x] Add unresolved branch recording and a final resolution pass based on bytecode offsets.
- [x] Convert `&&` and `||` lowering to the new branch infrastructure.
- [x] Add a dedicated conditional emission path for boolean expressions.
- [x] Implement primitive `==` and `!=` lowering.
- [x] Implement reference `==` and `!=` lowering.
- [x] Optimize `null` comparisons to `ifnull` and `ifnonnull`.
- [x] Implement boolean materialization for comparison expressions.
- [x] Implement `Stmt::If` bytecode emission.
- [ ] Implement `Stmt::While` bytecode emission.
- [ ] Implement `Stmt::DoWhile` bytecode emission.
- [ ] Implement `Stmt::For` bytecode emission.
- [ ] Add a control-flow stack for `break`, `continue`, and labeled statements.
- [x] Fix method invocation descriptor generation to include return types.
- [ ] Add verification sources that exercise equality inside `if` statements and loops.
- [x] Run `cargo run -p verification` and compare the output.
- [x] Run `./scripts/check-code.sh`.
