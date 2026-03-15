# What is the plan for turning unsupported bytecode-generation features into explicit exceptions and diagnostics?

## Why is a dedicated plan needed?

The current bytecode generator still contains several partial or placeholder paths for unsupported language features.
Some cases silently emit no bytecode, some emit placeholder constant-pool indexes such as `0`, and some rely on `unreachable!()` for states that can still be reached by incomplete frontend support.

That behavior is too weak for a compiler frontend.
When rajac accepts a source program but cannot lower part of it, the failure should be explicit and actionable.
The compiler should:

- emit a diagnostic that explains which feature is unsupported or unimplemented
- stop pretending the feature compiled successfully
- generate bytecode that fails loudly if execution still reaches the unsupported path

This is especially important in the bytecode stage, where silent no-ops can produce invalid or misleading class files.

## What is the current implementation baseline?

The current codebase already has:

- semantic diagnostics infrastructure in earlier stages
- bytecode-generation support for explicit `throw` statements and `athrow`
- pretty-print and verification infrastructure for inspecting generated class files

The current bytecode generator still has unsupported paths such as:

- `Stmt::Try { .. }` and `Stmt::Synchronized { .. }` in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs), which are still emitted as no-ops
- expression cases like `instanceof`, `newarray`, and `super` calls that still use placeholder constant-pool indexes or fallback behavior
- internal `unreachable!()` branches that should become controlled compiler failures if incomplete frontend states can still reach them

The exact set should be audited at implementation time rather than guessed from comments alone.

## What should the new behavior be?

Whenever bytecode generation encounters an unsupported or not-yet-implemented feature, rajac should produce both:

1. a source diagnostic explaining the unsupported feature
2. bytecode that throws a corresponding runtime exception if execution reaches that path

The first part preserves compiler usability.
The second part preserves runtime honesty for any successfully emitted class file that still contains unsupported lowering stubs.

The runtime exception should carry a message that matches or closely mirrors the diagnostic so failures are easy to correlate.

## What shared mechanism should be introduced?

The bytecode generator should gain a dedicated helper for unsupported features, implemented in [bytecode.rs](/data/projects/rajac/crates/bytecode/src/bytecode.rs).

That helper should centralize:

- constructing a stable unsupported-feature message
- adding the required constant-pool entries
- emitting bytecode to instantiate and throw a runtime exception
- keeping operand-stack accounting correct

The helper should probably emit:

1. `new java/lang/UnsupportedOperationException`
2. `dup`
3. load the message string
4. `invokespecial <init>(Ljava/lang/String;)V`
5. `athrow`

If another exception type proves more appropriate, the plan should still keep the helper centralized so all unsupported features behave consistently.

## How should diagnostics be surfaced?

The bytecode stage currently returns `RajacResult` errors rather than stage-specific semantic diagnostics.
This plan should make the failure mode explicit without weakening existing stage boundaries.

The first implementation should choose one clear path and use it consistently:

- either attach a diagnostic to the compilation unit before generation aborts
- or return a generation-stage error that includes the source context and the same unsupported-feature message

Because the user explicitly asked for both an error message and a diagnostic, the implementation should prefer the first option if the current stage plumbing can support it cleanly.
If the existing generation API cannot currently attach structured diagnostics, the plan should introduce the minimum plumbing necessary instead of duplicating string formatting ad hoc at each call site.

## Which current unsupported cases should be migrated first?

The first sweep should focus on cases that are definitely placeholders today and can otherwise miscompile silently.

That includes at least:

- `Stmt::Try`
- `Stmt::Synchronized`
- placeholder instruction emission for `instanceof`
- placeholder instruction emission for array creation paths that still use dummy indexes
- placeholder `super` call emission if it still uses unresolved method references

The implementation should also review any remaining `unreachable!()` in bytecode generation and decide case by case whether they represent:

- true internal invariants that should stay hard assertions
- or unsupported source-feature states that should be converted to the shared unsupported-feature path

## What should be avoided?

The first implementation should not silently “recover” by skipping code generation for unsupported constructs.
It should also avoid scattering message construction across many match arms.

The plan should avoid:

- leaving unsupported features as empty statement handlers
- emitting malformed or placeholder constant-pool references in production output
- mixing multiple exception types or message formats for equivalent unsupported cases
- turning real compiler bugs into user-facing unsupported-feature messages when the situation is actually an internal invariant violation

## What architecture changes should accompany the work?

This work should keep unsupported-feature handling explicit and reusable.

The implementation should introduce:

- a bytecode helper such as `emit_unsupported_feature(...)`
- a small message-formatting helper or enum for unsupported feature kinds
- generation-stage source-context plumbing if needed for diagnostics
- focused tests that assert both the thrown bytecode shape and the surfaced error message

If the helper needs source markers, the code generator should accept just enough statement or expression context to point at the failing construct without turning the entire bytecode layer into a diagnostics framework.

## What is the recommended implementation order?

1. Audit the current bytecode generator for empty, placeholder, and miscompiling unsupported paths.
2. Introduce a shared helper that emits `UnsupportedOperationException` bytecode with a message.
3. Introduce or extend generation-stage diagnostics so the same unsupported message is surfaced during compilation.
4. Replace silent no-op handlers for unsupported statements with the shared helper.
5. Replace placeholder instruction paths for unsupported expressions with the shared helper.
6. Review remaining `unreachable!()` usage in bytecode generation and convert the user-reachable ones.
7. Add focused unit tests for the helper and the migrated unsupported cases.
8. Add or update verification fixtures if the generated class-file shape for unsupported cases is intentionally checked.
9. Run verification and repository-wide checks.

## What tests and verification should be added?

Tests should be colocated with bytecode generation where the unsupported lowering behavior is implemented.

The first test set should include:

- unsupported statement lowering emits bytecode that constructs and throws `UnsupportedOperationException`
- unsupported expression lowering emits the same exception shape
- the emitted message string includes the unsupported feature name
- generation-stage reporting exposes a matching error message or diagnostic

Verification should be added only where it gives clear value.
Because unsupported features may intentionally stop successful compilation, some coverage may belong in invalid fixtures rather than valid bytecode fixtures.

The verification plan should consider:

- invalid fixtures for unsupported source constructs if rajac now rejects them during generation with stable source lines
- pretty-printed class-file inspection only for cases where rajac intentionally still emits a runtime-throwing stub

If verification coverage is intentionally limited because generation now aborts before classfile emission, the plan should say so explicitly in the implementation updates.

## What assumptions and scope boundaries should stay explicit?

This plan assumes:

- unsupported feature handling is primarily a bytecode-generation concern for now
- earlier stages may still parse and semantically validate constructs that generation cannot yet lower
- `UnsupportedOperationException` is an acceptable runtime exception type for generated stubs
- some `unreachable!()` sites should remain internal assertions if they truly represent compiler invariants

This plan does not attempt to implement the unsupported features themselves.
It only makes unsupported behavior explicit, diagnosable, and non-silent.

## What completion criteria should define success?

This unsupported-feature handling milestone should be considered complete when:

- bytecode generation has a shared helper for unsupported-feature exception emission
- currently silent unsupported statement and expression cases use that helper
- generation surfaces a corresponding error message and diagnostic for unsupported features
- no known user-reachable unsupported path still compiles as a silent no-op or with placeholder constant-pool indexes
- colocated tests cover the helper and migrated unsupported cases
- `cargo run -p verification --bin verification` passes if verification remains applicable
- `./scripts/check-code.sh` passes

## What checklist tracks the work?

- [ ] Audit current bytecode-generation placeholders and silent unsupported paths.
- [ ] Add a shared bytecode helper for throwing `UnsupportedOperationException` with a message.
- [ ] Add or extend generation-stage diagnostics for unsupported features.
- [ ] Replace unsupported statement no-op handlers with the shared helper.
- [ ] Replace unsupported expression placeholder paths with the shared helper where appropriate.
- [ ] Review user-reachable `unreachable!()` sites in bytecode generation and convert the unsupported ones.
- [ ] Add colocated tests for unsupported-feature exception emission and reporting.
- [ ] Add or update verification fixtures or explicitly document why verification is limited for generation-stage failures.
- [ ] Run `cargo run -p verification --bin verification` if applicable.
- [ ] Run `./scripts/check-code.sh`.
