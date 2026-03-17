# What problem is this plan solving?

The compiler now performs parsing, collection, resolution, attribute analysis, and generation, but it still lacks a dedicated flow-analysis phase.
That leaves an important quality gap: rajac can type-check many programs without proving that local variables are definitely assigned before use.

This is the next high-leverage milestone because it strengthens semantic correctness across the compiler rather than adding one more isolated language feature.
It also creates cleaner boundaries between semantic validation and code generation.

# What is the current gap?

Today [attribute_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/attribute_analysis.rs) already handles:

- local and parameter binding
- assignment compatibility
- core expression typing
- control-flow legality for `break`, `continue`, `switch`, `throw`, and reachability in straightforward abrupt-completion cases

However, the compiler still does not implement a real flow-analysis pass for:

- definite assignment of locals before read
- path-sensitive merge behavior across `if` and loop edges
- constructor and method body assignment-state tracking as a first-class phase
- a stable architectural home for future flow-sensitive checks

The pipeline module in [mod.rs](/data/projects/rajac/crates/compiler/src/stages/mod.rs) also still stops at attribute analysis and generation, while [compilation-pipeline.md](/data/projects/rajac/docs/compilation-pipeline.md) describes a separate flow-analysis stage.

# What implementation approach should be used?

Introduce a dedicated `flow_analysis` stage in `crates/compiler/src/stages` and run it after attribute analysis and before generation.

The first milestone should stay deliberately narrow.
It should focus on definite assignment for locals and parameters rather than trying to implement every JLS flow rule at once.

The stage should:

- build a flow state for each method, constructor, static block, and instance initializer body
- treat parameters as definitely assigned at entry
- treat local variables as unassigned until their initializer completes successfully
- propagate assignment state through blocks, `if`, loops, `switch`, `try`, and abrupt control transfer conservatively
- reject identifier reads that are not definitely assigned on all incoming paths

This work should prefer a small, explicit flow-state model over scattering new checks through attribute analysis.
Attribute analysis should remain the owner of typing and statement legality, while flow analysis should own path-sensitive assignment facts.

# What should remain out of scope for this milestone?

This milestone should not try to solve the full Java flow-analysis surface in one pass.

The first implementation should keep these out of scope unless they fall out naturally from the core framework:

- definite unassignment for `final` locals
- blank `final` field rules
- full checked-exception flow
- pattern variables
- try-with-resources data-flow rules
- exhaustive switch-expression flow
- every subtle JLS reachability rule already partially handled in attribute analysis

Those are better follow-up milestones once the flow framework is stable and tested.

# How should the flow state be modeled?

The stage should introduce dedicated helper types rather than open-coded sets.

The first implementation should add:

- a flow-analysis context that knows the current body kind and diagnostic sink
- an assignment-state representation keyed by local identity rather than raw names when practical
- merge helpers for branch joins
- transfer helpers for statements and expressions that can affect assignment state
- a small result type that distinguishes normal completion from abrupt completion so merges stay explicit

If the current AST does not yet carry a stable local identity, the first milestone can key flow state by source-local declaration identity derived from the AST arena.
That tradeoff should be explicit in the implementation.

# How should definite assignment be checked?

Definite assignment should be enforced at variable-read sites, not only at declarations.

The first rules should include:

- parameters are definitely assigned at method and constructor entry
- reading a local before assignment is an error
- a local declared without initializer is not definitely assigned
- a local declared with initializer becomes definitely assigned only after the initializer completes
- assignment expressions mark the target local as definitely assigned after the right-hand side is analyzed
- branch joins require the variable to be assigned on all incoming normal-completion paths

The first implementation should support ordinary identifier reads before expanding to more specialized cases.

# How should control-flow constructs transfer assignment state?

The flow framework should start with a conservative, explicit transfer model.

The first milestone should cover:

- block sequencing
- `if` / `if-else`
- `while`, `do-while`, and `for`
- `switch`
- `return`, `throw`, `break`, and `continue`
- `try` / `catch` / `finally`

Where precise loop fixed-point behavior would be expensive or complex, the initial implementation should prefer sound conservative results over aggressive acceptance.
It is better to reject a narrow edge case temporarily than to accept definitely-invalid code.

# How should diagnostics be designed?

Flow-analysis diagnostics should use the existing `rajac_diagnostics::Diagnostic` infrastructure and should stay distinct from type-checking errors.

The first diagnostic set should cover:

- variable might not have been initialized
- reading a local before definite assignment

Messages should prefer stable wording that is easy to verify against OpenJDK by line number.
If rajac intentionally uses clearer wording than OpenJDK, add an override in [verification_main.rs](/data/projects/rajac/crates/verification/src/verification_main.rs).

# What architecture changes should accompany the work?

This milestone should move the compiler architecture closer to the documented pipeline instead of further enlarging attribute analysis.

The implementation should:

- add `flow_analysis.rs` to [mod.rs](/data/projects/rajac/crates/compiler/src/stages/mod.rs)
- wire the new phase into [compiler.rs](/data/projects/rajac/crates/compiler/src/compiler.rs) between attribute analysis and generation
- keep new flow helper structs in dedicated files if `flow_analysis.rs` starts growing too large
- avoid duplicating type-checking logic that already belongs to attribute analysis
- document any non-obvious transfer-function decisions with concise RustDoc or hyperlit comments when needed

# What order should the implementation follow?

1. Add this plan in `docs/plans`.
2. Define the flow-analysis stage entry point and wire it into the compiler pipeline.
3. Introduce flow-state and body-result helper types.
4. Track local declarations and parameter initialization state at body entry.
5. Reject reads of unassigned locals in straight-line code.
6. Add branch-merge behavior for `if` / `if-else`.
7. Add conservative loop handling for `while`, `do-while`, and `for`.
8. Add transfer behavior for `switch`, `break`, `continue`, `return`, and `throw`.
9. Add conservative handling for `try` / `catch` / `finally`.
10. Add colocated tests for successful and failing definite-assignment cases.
11. Add invalid verification fixtures under `verification/sources_invalid/typecheck`.
12. Add or update verification message overrides if rajac wording intentionally differs from OpenJDK.
13. Run `cargo run -p rajac-verification --bin verification`.
14. Run `./scripts/check-code.sh`.
15. Move the completed plan to `docs/plans/completed`.

# What assumptions matter for this work?

- Attribute analysis will continue to own expression typing, assignment compatibility, and control-flow legality.
- The AST and arena provide enough identity information to track locals deterministically through a method body.
- Conservative flow results are acceptable for the first milestone if they prevent unsound acceptance.
- Verification fixtures under `verification/sources_invalid/typecheck` are the right compatibility signal for these diagnostics.

# How will this work be verified?

Verification should cover both local flow behavior and end-to-end compiler diagnostics.

The expected verification work is:

- add colocated unit tests for definite-assignment transfer and merge behavior
- add invalid verification fixtures for uninitialized local reads in straight-line, branch, and loop cases
- run `cargo test` for the affected crate or targeted module tests
- run `cargo run -p rajac-verification --bin verification`
- run `./scripts/check-code.sh`

# What concrete work items are planned?

- [x] Create this plan in `docs/plans`.
- [x] Add a dedicated flow-analysis stage module and wire it into the compiler pipeline.
- [x] Introduce flow-state and completion-result helper types.
- [x] Track definite assignment for parameters and local declarations.
- [x] Reject reads of locals that are not definitely assigned.
- [x] Implement branch merge behavior for `if` / `if-else`.
- [x] Implement conservative loop flow for `while`, `do-while`, and `for`.
- [x] Implement transfer behavior for `switch`, `break`, `continue`, `return`, and `throw`.
- [x] Implement conservative flow handling for `try` / `catch` / `finally`.
- [x] Add colocated tests for definite assignment diagnostics and successful cases.
- [x] Add or update invalid verification fixtures under `verification/sources_invalid/typecheck`.
- [x] Add or update verification error-message overrides if needed.
- [x] Run `cargo run -p rajac-verification --bin verification`.
- [x] Run `./scripts/check-code.sh`.
