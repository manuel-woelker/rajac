# What problem is this plan solving?

The compiler now has a dedicated flow-analysis stage with conservative definite-assignment checks for locals and parameters.
The next semantic gap in that area is Java's `final` variable discipline.

Without `final` flow rules, rajac can still accept programs that reassign `final` locals or fail to assign them exactly once before use.
That weakens semantic correctness and leaves an important part of Java's path-sensitive validation unimplemented.

# What is the current gap?

The current flow-analysis stage in [flow_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/flow_analysis.rs) tracks whether locals are definitely assigned.
It does not yet track whether a variable is:

- declared `final`
- still definitely unassigned
- assigned more than once
- subject to the stricter initialization rules expected for `final` locals

That means the new flow framework is in place, but one of the most important consumers of that framework is still missing.

# What implementation approach should be used?

Extend the existing `flow_analysis` stage so local state records both assignment status and `final` semantics.

The first milestone should focus on `final` locals and `final` parameters only.
It should not immediately expand into every field-initialization rule.

The stage should:

- record whether a local or parameter is `final`
- reject assignment to a `final` local or parameter after it has already been assigned
- require `final` locals without initializer to be assigned exactly once before read
- preserve conservative merge behavior across branches, loops, `switch`, and `try`

This work should build on the current flow state instead of adding special-case checks in attribute analysis or bytecode generation.

# What should remain out of scope for this milestone?

The first implementation should stay focused on local-variable semantics.

The following should remain out of scope unless they fall out naturally from the design:

- blank `final` field rules for constructors
- `final` instance-field initialization across constructor chains
- static blank `final` field initialization rules
- full constant-variable classification
- capture rules for inner classes and lambdas
- every subtle JLS corner case around unreachable code and `final`

Blank `final` fields should be the next follow-up milestone once local rules are stable.

# How should local flow state change?

The local-state model should carry enough information to distinguish ordinary locals from `final` locals.

The first implementation should add:

- an `is_final` flag for tracked locals
- a representation of whether the local has already been assigned
- helper methods that distinguish first assignment from illegal reassignment
- merge behavior that remains conservative when different control-flow paths disagree

If the current AST does not expose modifiers directly on local declarations, the implementation should first add the minimum AST plumbing needed to carry `final` on local declarations and parameters.
That plumbing should stay narrow and well documented.

# How should `final` local rules be enforced?

The first rules should include:

- a `final` local with an initializer is assigned at declaration time
- a `final` local without an initializer starts definitely unassigned
- the first assignment to a definitely unassigned `final` local is allowed
- any later assignment to that `final` local is an error
- a `final` parameter is considered assigned at method or constructor entry and cannot be reassigned
- reading a `final` local before definite assignment is still an error under the existing maybe-uninitialized rule

The implementation should cover both direct `=` assignments and mutating operations that imply assignment, such as increment and decrement.

# How should control-flow merges behave for `final` locals?

The stage should continue to prefer sound conservative behavior.

The first merge rules should be:

- if one branch assigns a `final` local and another does not, the join result should not treat it as definitely assigned
- if both branches assign the same definitely unassigned `final` local exactly once, the join result can treat it as definitely assigned
- if any path proves that a second assignment occurred, that path should emit an error at the second assignment site
- loop bodies should remain conservative unless the analysis can prove a single assignment on all normal paths

This means the initial implementation may reject some edge cases that OpenJDK accepts, but it must not accept invalid programs that reassign `final` locals.

# How should diagnostics be designed?

Diagnostics should remain stable and specific enough for verification.

The first diagnostic set should cover:

- cannot assign a value to final variable `<name>`
- variable `<name>` might not have been initialized

When possible, wording should stay close to OpenJDK so verification can compare by line number and message with minimal overrides.
If rajac intentionally improves the wording, add an override in [verification_main.rs](/data/projects/rajac/crates/verification/src/verification_main.rs).

# What architecture changes should accompany the work?

This milestone should reinforce the role of `flow_analysis` as the path-sensitive semantic owner.

The implementation should:

- extend the existing flow-state types in [flow_analysis.rs](/data/projects/rajac/crates/compiler/src/stages/flow_analysis.rs)
- keep modifier/plumbing changes minimal and colocated with the AST or parser code that needs them
- avoid duplicating `final` checks in attribute analysis
- add concise RustDoc or hyperlit comments only where a non-obvious flow-transfer decision needs rationale

# What order should the implementation follow?

1. Add this plan in `docs/plans`.
2. Confirm how local and parameter modifiers are represented today, and add minimal AST/parser support for `final` locals if needed.
3. Extend flow local-state tracking to record `final` semantics.
4. Reject reassignment of `final` parameters.
5. Reject reassignment of `final` locals after their first successful assignment.
6. Ensure increment and decrement respect `final` restrictions.
7. Preserve conservative branch and loop merge behavior for `final` locals.
8. Add colocated tests for valid single-assignment and invalid reassignment cases.
9. Add invalid verification fixtures under `verification/sources_invalid/typecheck`.
10. Add or update verification error-message overrides if needed.
11. Run `cargo run -p rajac-verification --bin verification`.
12. Run `./scripts/check-code.sh`.
13. Move the completed plan to `docs/plans/completed`.

# What assumptions matter for this work?

- The new flow-analysis stage remains the correct place for path-sensitive `final` checks.
- Conservative handling is acceptable for the first milestone if it avoids unsound acceptance.
- Parser and AST changes for local `final` modifiers can stay isolated and do not require broader modifier refactors.
- Verification fixtures under `verification/sources_invalid/typecheck` are the right compatibility mechanism for these diagnostics.

# How will this work be verified?

Verification should cover both focused flow behavior and end-to-end diagnostics.

The expected verification work is:

- add colocated unit tests for `final` local and parameter flow behavior
- add valid verification fixtures for successful `final` assignments in different branches
- add invalid verification fixtures for reassignment of `final` locals and parameters
- add invalid verification fixtures for maybe-uninitialized `final` locals where needed
- run targeted crate tests or `cargo test` for the affected compiler code
- run `cargo run -p rajac-verification --bin verification`
- run `./scripts/check-code.sh`

# What concrete work items are planned?

- [x] Create this plan in `docs/plans`.
- [x] Add minimal AST/parser support for `final` local declarations if the current representation lacks it.
- [x] Extend flow local-state tracking with `final` semantics.
- [x] Reject reassignment of `final` parameters.
- [x] Reject reassignment of `final` locals after first assignment.
- [x] Reject increment and decrement on `final` locals and parameters.
- [x] Preserve conservative merge behavior for `final` locals across branches and loops.
- [x] Add colocated tests for valid and invalid `final` local flow.
- [x] Add valid verification fixtures for successful `final` assignments in different branches.
- [x] Add or update invalid verification fixtures under `verification/sources_invalid/typecheck`.
- [x] Add or update verification error-message overrides if needed.
- [x] Run `cargo run -p rajac-verification --bin verification`.
- [x] Run `./scripts/check-code.sh`.
