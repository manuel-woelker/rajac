# What problem is this plan solving?

The compiler now enforces blank `final` instance-field initialization in constructor bodies, including same-class delegation through `this(...)`.
The next semantic gap is initialization that happens before the constructor body runs: instance field initializers and instance initializer blocks.

Without this milestone, rajac can reject valid code that initializes blank `final` fields outside the constructor body, or accept invalid code if constructor analysis does not model initialization order precisely enough.

# What is the current gap?

The current flow-analysis stage in [flow_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/flow_analysis.rs) tracks blank `final` instance fields through constructor execution.
It does not yet fully model all same-class initialization mechanisms that contribute to a constructor's starting state.

The missing pieces are:

- blank `final` fields assigned by instance field initializers
- blank `final` fields assigned by instance initializer blocks
- duplicate assignment caused by both an initializer and a constructor assigning the same blank `final` field
- ordering between instance initializers and constructor-body analysis

This means the compiler is not yet sound for classes that rely on Java's full per-class instance-initialization sequence rather than constructor bodies alone.

# What implementation approach should be used?

Extend constructor-related flow analysis so each constructor starts from the effects of the class's instance initialization sequence.

The first milestone should focus on:

- collecting blank `final` field assignments from instance field initializers
- collecting blank `final` field assignments from instance initializer blocks
- applying those effects before analyzing constructor-body statements after any explicit or implicit constructor prologue
- rejecting duplicate assignment when a blank `final` field is written in both an initializer and later in the same constructor chain

This work should stay centered in `flow_analysis`, with only the minimum parser, AST, or earlier-stage support needed to expose initializer constructs consistently.

# What should remain out of scope for this milestone?

This milestone should stay focused on same-class initialization semantics.

The following should remain out of scope unless required for correctness in the supported subset:

- superclass-owned field analysis across inheritance
- static `final` field initialization rules
- exception-sensitive initialization flow through throwing initializers
- helper-method side effects during initialization
- record-specific initialization semantics

If any unsupported initializer shape would otherwise compile unsoundly, it should fail conservatively or be diagnosed explicitly.

# How should same-class instance initialization be modeled?

Java instance construction has a defined order within a class, and the compiler needs a corresponding model.

The first implementation should model a per-class initialization prefix that runs before the constructor body:

- instance field initializers in declaration order
- instance initializer blocks in source order relative to fields
- then the constructor body after constructor prologue handling

The model should preserve exact source ordering where practical, because duplicate-assignment diagnostics for blank `final` fields depend on which write happens first.

# How should initializer effects interact with constructor analysis?

The first supported rule set should be:

- a blank `final` field assigned in an instance field initializer is definitely assigned before the constructor body starts
- a blank `final` field assigned in an instance initializer block is definitely assigned before the constructor body starts
- if a constructor body assigns a tracked blank `final` field that was already assigned by initializers, that is an error
- if initializers do not definitely assign a tracked blank `final` field, the constructor chain still has to assign it on all normal paths
- if two initializers assign the same tracked blank `final` field in the same construction path, that is an error

This preserves the Java rule that a blank `final` field must be assigned exactly once across the full per-class instance-initialization sequence.

# What architecture changes should accompany the work?

This milestone should deepen the constructor and field flow model without spreading ownership across unrelated stages.

The implementation should:

- extend [flow_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/flow_analysis.rs) to analyze initializer-originated assignments
- reuse existing blank `final` field state tracking rather than introducing a second parallel mechanism
- keep constructor-ordering validation in attribute analysis only if flow analysis does not own it already
- add concise comments only where Java initialization ordering is non-obvious

# What order should the implementation follow?

1. Add this plan in `docs/plans`.
2. Identify how instance field initializers and instance initializer blocks appear in the AST and resolved representation.
3. Build a per-class ordered instance-initialization sequence for flow analysis.
4. Apply initializer effects to the blank `final` field state before constructor-body analysis.
5. Reject duplicate blank `final` field assignment across initializers and constructor bodies.
6. Require tracked blank `final` fields to be initialized across the full same-class initialization sequence on normal completion.
7. Add colocated tests for successful initialization through field initializers and instance initializer blocks.
8. Add colocated tests for duplicate assignment between initializers and constructors.
9. Add valid verification fixtures for blank `final` fields initialized by field initializers and initializer blocks.
10. Add invalid verification fixtures for missing assignment and duplicate assignment involving initializers.
11. Add or update verification error-message overrides if needed.
12. Run `cargo run -p rajac-verification --bin verification`.
13. Run `./scripts/check-code.sh`.
14. Move the completed plan to `docs/plans/completed`.

# What assumptions matter for this work?

- The parser and AST already represent instance field initializers and initializer blocks, or can be extended without broad grammar work.
- Flow analysis remains the right owner for blank `final` assignment tracking across all same-class initialization mechanisms.
- Conservative treatment is acceptable for initializer shapes that are not yet fully modeled, as long as rajac does not accept unsound code.
- Verification fixtures under `verification/sources` and `verification/sources_invalid/typecheck` remain the right compatibility mechanism for this milestone.

# How will this work be verified?

Verification should cover focused initialization-order behavior as well as end-to-end compiler output.

The expected verification work is:

- add colocated unit tests for field-initializer and initializer-block effects on blank `final` field flow
- add valid verification fixtures for successful blank `final` initialization through instance field initializers
- add valid verification fixtures for successful blank `final` initialization through instance initializer blocks
- add invalid verification fixtures for duplicate assignment involving initializers and constructors
- add invalid verification fixtures for missing blank `final` initialization when initializers are partial
- run targeted crate tests or `cargo test` for the affected compiler code
- run `cargo run -p rajac-verification --bin verification`
- run `./scripts/check-code.sh`

# What concrete work items are planned?

- [x] Create this plan in `docs/plans`.
- [x] Identify and model instance field initializers and instance initializer blocks in flow analysis.
- [x] Apply initializer effects before constructor-body blank `final` analysis.
- [x] Reject duplicate blank `final` field assignment across initializers and constructors.
- [x] Require tracked blank `final` fields to be initialized across the full same-class initialization sequence.
- [x] Add colocated tests for valid and invalid blank `final` initialization through initializers.
- [x] Add valid verification fixtures for successful initialization via field initializers and initializer blocks.
- [x] Add or update invalid verification fixtures under `verification/sources_invalid/typecheck` for initializer-related blank `final` errors.
- [x] Add or update verification error-message overrides if needed.
- [x] Run `cargo run -p rajac-verification --bin verification`.
- [x] Run `./scripts/check-code.sh`.
