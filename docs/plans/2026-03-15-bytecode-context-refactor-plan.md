# What is the plan for tightening bytecode-generation contracts with context types?

## Why is this refactor worth doing?

The bytecode and classfile layers still rely on functions that pass several related pieces of state in parallel:

- AST access
- type information
- symbol lookup
- constant-pool mutation
- unsupported-feature reporting

That shape is error-prone even when it compiles.
It makes call sites noisy, increases the chance of stale argument lists during refactors, and forces Clippy-silencing or ad hoc context structs only where warnings already fired.

The recent unsupported-feature work already exposed that pressure in [classfile.rs](/data/projects/rajac/crates/bytecode/src/classfile.rs), where plumbing extra state immediately pushed several functions over the argument-count limit.
The next quality step is to make those dependencies explicit and local by introducing small context types with clear ownership.

## What problem should this plan solve precisely?

This work should improve maintainability and reduce plumbing bugs.
It is not primarily a feature change.

Success means:

- fewer functions need long parallel argument lists
- bytecode-generation dependencies are grouped by responsibility
- stateful mutation points become easier to identify
- future feature work can add one field to a context instead of widening many signatures
- tests still cover the current behavior without widening the public API unnecessarily

## What is the current baseline?

The codebase already has one small step in this direction:

- `ClassfileGenerationContext` in [classfile.rs](/data/projects/rajac/crates/bytecode/src/classfile.rs)

That context is useful, but it is still only a partial adaptation.
Other parts of the bytecode pipeline still pass multiple related values separately, especially around:

- constant-pool mutation in classfile emission
- method and constructor lowering helpers
- generation-stage report plumbing
- bytecode helper functions that repeatedly receive type and symbol context

The main design risk right now is inconsistency rather than one single broken abstraction.

## What design direction should the refactor take?

The refactor should prefer a few narrow context types instead of one large “everything bagel” struct.

The most useful separation is by responsibility:

1. immutable semantic lookup context
2. mutable classfile emission context
3. mutable per-method bytecode emission context

That likely means introducing or refining types along these lines:

- a lookup-focused context for `AstArena`, `TypeArena`, and `SymbolTable`
- a classfile-generation context that owns or borrows the constant pool plus unsupported-feature collection
- helper methods on those context types so internal functions operate on `self` instead of repeatedly taking parallel state

The exact names can change, but the API should make it obvious which layer owns which mutation.

This refactor should also treat file and module boundaries as part of the design.
If the context cleanup is left entirely inside the current large files, the code may still remain harder to navigate than it needs to be.
Module splits are in scope when they sharpen responsibility boundaries and reduce file size.

## Which concrete seams should be targeted first?

The first pass should stay close to the currently active friction points.

### What should happen in `classfile.rs`?

The functions in [classfile.rs](/data/projects/rajac/crates/bytecode/src/classfile.rs) should be the first target because they already show the problem clearly.

The refactor should:

- review `ClassfileGenerationContext` and decide whether it should also carry `ConstantPool` access through methods instead of passing `&mut ConstantPool` to many helpers
- reduce helper signatures like `method_from_ast(...)` and `constructor_from_ast(...)` further by moving repeated dependencies behind context methods
- decide whether top-level free functions such as `field_from_ast`, `method_to_descriptor`, and `exceptions_attribute_from_ast_types` should become methods on a context or builder type

The goal is not to remove every helper function.
The goal is to make helper boundaries align with state ownership.

This is also the best place to split modules, because the responsibilities are already distinct enough to separate without fighting borrow-heavy control flow.

A good target layout is:

- `crates/bytecode/src/classfile/mod.rs` for public entry points and module wiring
- `crates/bytecode/src/classfile/generation_context.rs` for `ClassfileGenerationContext` and closely related result types
- `crates/bytecode/src/classfile/classfile_builder.rs` for class-level assembly
- `crates/bytecode/src/classfile/member_lowering.rs` for field, method, and constructor lowering
- `crates/bytecode/src/classfile/descriptor.rs` for descriptor construction helpers
- `crates/bytecode/src/classfile/attributes.rs` for `Exceptions` and `InnerClasses` attribute assembly
- `crates/bytecode/src/classfile/naming.rs` for internal-name resolution helpers

The refactor does not need to land in exactly that shape, but it should move toward similarly clear module ownership.

### What should happen in `bytecode.rs`?

The code generator in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs) already centralizes a lot of mutable state inside `CodeGenerator`.
That is good, but there is still room to tighten contracts around helper methods that effectively rely on the same semantic environment.

This pass should review whether any free helper functions are really methods-in-disguise, especially functions that repeatedly depend on:

- `TypeArena`
- `SymbolTable`
- descriptor construction
- type-kind classification

Not every pure helper needs to move.
Keep pure, obviously stateless helpers free-standing when they are genuinely simpler that way.

Module splitting here should be more conservative than in `classfile.rs`, because `CodeGenerator` owns a lot of mutable state and aggressive splitting can make borrowing and navigation worse rather than better.

A reasonable target layout is:

- `crates/bytecode/src/bytecode/mod.rs` for module wiring and public exports
- `crates/bytecode/src/bytecode/code_generator.rs` for the core `CodeGenerator` type
- `crates/bytecode/src/bytecode/control_flow.rs` for statement and branching helpers that are primarily about labels and control-flow stacks
- `crates/bytecode/src/bytecode/expressions.rs` for expression lowering helpers
- `crates/bytecode/src/bytecode/locals.rs` for local-slot and local-kind handling
- `crates/bytecode/src/bytecode/type_helpers.rs` for descriptor and type-shape helpers that remain worth keeping outside the core generator
- `crates/bytecode/src/bytecode/unsupported.rs` for unsupported-feature reporting and stub emission support

The key rule is to split by ownership and mutation boundaries, not mechanically by AST variant count.
Helpers that fundamentally require `&mut CodeGenerator` should remain methods on that type, even if they live in a focused submodule.

### What should happen in the compiler generation stage?

The diagnostics plumbing in [generation.rs](/data/projects/rajac/crates/compiler/src/stages/generation.rs) should also be checked for contract clarity.

This part of the plan should:

- review whether `generate_classfiles(...)` returning `(usize, Diagnostics)` is the right long-term shape
- consider a small `GenerationResult` struct if that improves readability and reduces positional return-value mistakes
- keep source-marker to diagnostic conversion local and consistent rather than spreading it across future call sites

This is a smaller refactor than the classfile work, but it is part of the same contract-tightening goal.

This layer can also be split if the result and diagnostic responsibilities become clearer that way.
A reasonable target is:

- `crates/compiler/src/stages/generation/mod.rs` for the stage entry point
- `crates/compiler/src/stages/generation/diagnostics.rs` for source-marker to diagnostic conversion
- `crates/compiler/src/stages/generation/generation_result.rs` for a named result type if introduced
- `crates/compiler/src/stages/generation/emit.rs` for file-writing logic

## What should explicitly stay out of scope?

This plan should not:

- change bytecode semantics intentionally
- add support for new Java features
- redesign the full compiler pipeline
- introduce a giant shared context spanning parsing, resolution, analysis, and generation
- churn public APIs unless the simplification is clearly worth it
- split files mechanically without a responsibility-based boundary

The point is to improve local structure, not to impose a new architecture for the entire compiler.

## What implementation order is recommended?

1. Audit bytecode/classfile helpers with the highest argument counts or repeated dependency tuples.
2. Decide the minimum set of context/result types and the minimum useful module splits for this pass.
3. Refactor and split `classfile.rs` first, because it already has context plumbing and current Clippy pressure.
4. Replace positional tuple-style generation-stage results with a named result type if that improves clarity, and split generation-stage helpers only if the boundary is clean.
5. Review `bytecode.rs` free helpers and move only the ones that materially benefit from context ownership or from a focused submodule.
6. Add or update colocated tests where refactors change internal boundaries or helper behavior.
7. Run verification and repository-wide checks.

## What risks should be watched carefully?

The main risk is over-refactoring.

This code can easily become worse if the refactor:

- hides simple data flow behind too many tiny wrappers
- introduces borrow-checker complexity that makes changes harder, not easier
- turns pure helpers into methods without improving readability
- mixes immutable semantic lookup state with mutable emission state in the same broad context

A second risk is accidentally changing behavior while rearranging ownership.
That means tests should emphasize bytecode shape and diagnostics stability rather than trusting the refactor by inspection.

A third risk is over-splitting.
Too many tiny files can make the code harder to follow than the current larger modules, especially around `CodeGenerator`.
The refactor should prefer a few meaningful responsibility-based modules over maximal fragmentation.

## How should this work be verified?

Verification should stay focused on behavior rather than the refactor itself.

This plan should include:

- existing colocated bytecode tests in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs)
- existing classfile tests in [classfile.rs](/data/projects/rajac/crates/bytecode/src/classfile.rs)
- generation-stage tests in [generation.rs](/data/projects/rajac/crates/compiler/src/stages/generation.rs) where return shapes or report plumbing change
- `cargo run -p verification --bin verification`
- `./scripts/check-code.sh`

New verification fixtures are probably not needed unless the refactor intentionally changes emitted bytecode or diagnostics.

## What assumptions should stay explicit?

This plan assumes:

- the current behavior is broadly correct and the main problem is maintainability
- the repository still prefers small, responsibility-focused helper types over broad shared state
- the bytecode crate can tolerate some internal API churn as long as behavior and tests remain stable
- Clippy pressure is a useful signal here, but not the sole design driver

## What completion criteria should define success?

This refactor should be considered complete when:

- the main bytecode/classfile helper seams no longer rely on avoidable long parallel argument lists
- context ownership is clearer in `classfile.rs`
- file and module boundaries better reflect responsibility where the split adds real clarity
- generation-stage result plumbing is readable and robust
- tests still cover the refactored paths
- `cargo run -p verification --bin verification` passes
- `./scripts/check-code.sh` passes

## What checklist should track the work?

- [ ] Audit high-friction bytecode and classfile helper signatures.
- [ ] Define the minimum context/result types and module splits needed for the refactor.
- [ ] Refactor and split `classfile.rs` to align helper boundaries with context ownership.
- [ ] Replace tuple-style generation-stage results with a named result type if it improves clarity, and split stage helpers if the result is cleaner.
- [ ] Refactor selected `bytecode.rs` helpers or submodules that materially benefit from context ownership.
- [ ] Add or update colocated tests affected by the refactor.
- [ ] Confirm verification fixtures do not need changes, or update them if behavior changed.
- [ ] Run `cargo run -p verification --bin verification`.
- [ ] Run `./scripts/check-code.sh`.
