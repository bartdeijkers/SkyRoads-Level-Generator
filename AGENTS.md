# General development rules

You are an engineer who writes code for **human brains, not machines**. Prefer code that is easy to understand, verify, and maintain. Human working memory is limited: readers can only hold a few facts in their head at once. When code forces them to track too many conditions, abstractions, or hidden assumptions, it becomes tiring to read and easy to break.

These rules apply everywhere, especially in new code and meaningful refactors. For trivial edits, generated code, or vendor code, follow them where practical and do not create churn by forcing broad rewrites.

Prefer code like this:
```
isValid = val > someConstant
isAllowed = condition2 || condition3
isSecure = condition4 && !condition5

if isValid && isAllowed && isSecure {
    ...
}
```

### Readability and flow
- Make conditionals readable. Extract dense expressions into intermediate variables with meaningful names.
- Prefer early returns over nested conditionals so readers can focus on the happy path.
- Stick to the smallest useful subset of the language. Readers should not need advanced language trivia to follow the code.
- Avoid unnecessary abstraction layers. A straight line of thought is usually easier to follow than jumping across many tiny wrappers.
- Prefer composition over deep inheritance. Do not force readers to chase behavior across multiple classes.
- Do not over-apply DRY. A little duplication is often cheaper than unnecessary shared dependencies.
- Use self-descriptive values and avoid custom mappings that require memorization.

### Functions
- Prefer small semantic functions with explicit inputs and explicit outputs. A function should do what its name says and nothing extra.
- Keep side effects out of semantic functions unless the side effect is the explicit purpose of the function.
- If a well-defined flow appears in multiple places, capture it in a clearly named function instead of re-explaining it with comments.
- Avoid shallow methods, classes, or modules with complex interfaces and trivial behavior. Prefer deeper units with simple interfaces and meaningful internal work.

### Orchestration
- Use pragmatic functions to orchestrate workflows, integrate side effects, and connect several semantic functions into one process.
- Pragmatic functions may contain more complex control flow, but they should stay readable and should not turn into vague utility buckets.
- Add doc comments only when they explain non-obvious behavior, constraints, failure modes, or important tradeoffs.
- Do not write "WHAT" comments that merely restate the next line of code. Use comments for "WHY", caveats, or a bird's-eye view of a block.

### Models
- Model data so that invalid states are difficult or impossible to represent.
- Use precise names and types. If a field does not clearly belong to the model's name, the model is probably too broad.
- Be suspicious of growing piles of optional fields. Split broad models into smaller concepts instead of turning them into loose bags of data.
- Prefer domain-specific types when identical shapes represent different concepts.