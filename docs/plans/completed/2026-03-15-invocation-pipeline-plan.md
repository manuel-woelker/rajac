# What problem is this plan solving?

The bytecode backend still guesses too much about method and constructor invocation.
It rebuilds owners and descriptors locally, always emits `invokevirtual` for ordinary
method calls, and still contains placeholder lowering for several invocation-related
expressions.

This is fragile and already caused stack-accounting regressions and verification
mismatches. The invocation pipeline should instead use resolved semantic data as the
source of truth for owner lookup, descriptor selection, opcode choice, and stack
effects.

# What is the current gap?

Today the backend:

- guesses method owners from receiver expression types or falls back to `java/lang/Object`
- emits `invokevirtual` for ordinary method calls regardless of static/interface/private dispatch
- handles constructors and `super(...)` through ad hoc logic rather than a shared invocation path
- still leaves unsupported invocation forms as placeholder bytecode in some paths

The compiler already resolves method ids for method calls and `super` calls, and the
symbol table already stores method signatures and modifiers. That semantic data should
drive bytecode emission directly.

# What implementation approach should be used?

Introduce an explicit backend invocation model inside the bytecode crate and make
method, constructor, and super-call lowering go through it.

The implemented work now:

- computes owner internal names from resolved method signatures and current class context
- chooses invocation opcode from semantic facts (`static`, `private`, `super`, interface owner)
- uses descriptor strings built from resolved signatures rather than expression-shape inference
- reuses the same descriptor-aware stack accounting for all call instructions
- emits `super` method calls through the same invocation helpers instead of a separate unsupported path

# What order should the implementation follow?

1. Add a plan document and commit it.
2. Refactor bytecode generation to carry current class invocation context.
3. Replace ordinary method-call lowering with resolved-signature-based invocation metadata.
4. Route `super(...)`, constructor calls, and default constructors through the same invocation helpers.
5. Add colocated tests for opcode selection, owners, and descriptors.
6. Run verification and repository-wide checks.
7. Move the finished plan to `docs/plans/completed`.

# What assumptions mattered for this work?

- Method resolution continues to populate `method_id` for `MethodCall` and `SuperCall`.
- Constructors remain represented in the symbol table as methods named after their class.
- Placeholder bytecode paths such as `new`, `instanceof`, and array creation still need
  follow-up work outside the scope of this invocation-pipeline pass.
- Verification fixtures under `verification/sources` are the correct compatibility signal for
  bytecode shape changes in this area.

# How will this work be verified?

Verification covered both local backend behavior and full compiler output.

- Added colocated tests in `crates/bytecode/src/bytecode.rs` for `invokestatic`,
  `invokeinterface`, and implicit private-method `invokespecial`.
- Ran `cargo test -p rajac-bytecode`.
- Ran `cargo run -p rajac-verification --bin verification`.
- Ran `./scripts/check-code.sh`.

# What concrete work items are planned?

- [x] Create this plan in `docs/plans`.
- [x] Add backend invocation metadata/helpers for owner, descriptor, and opcode selection.
- [x] Track current class context during bytecode generation so implicit and `super` calls have a stable owner.
- [x] Refactor ordinary method-call lowering to use resolved method signatures and modifiers.
- [x] Refactor constructor/default-constructor/super-constructor lowering to use shared invocation helpers.
- [x] Add or update colocated invocation tests.
- [x] Run `cargo test -p rajac-bytecode`.
- [x] Run `cargo run -p rajac-verification --bin verification`.
- [x] Run `./scripts/check-code.sh`.
