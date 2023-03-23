# Authoring Plugins

Hakana uses a plugin system based around Rust traits. Building plugins requires hooking into core Hakana hooks, and we do that by overriding trait methods.

When you create a plugin you need to create a Rust `struct` with a couple of `impl` corresponding to two traits, `CustomHook` and `InternalHook`:

```
pub struct YourPlugin {}

// some necessary boilerplate
impl CustomHook for YourPlugin {}

impl InternalHook for YourPlugin {
    // method overrides will go here
}
```

There are currently four hook methods you can use:

## after_stmt_analysis

This hook is called every time a statement is analyzed. There are many different kinds of statement in Hack — `if` statements, `return` statements, expression statements and many more.

A statement can be most easily thought of as any element you can put a semicolon after without changing the behaviour of your code.

They are distinct from expressions and definitions (functions, classes, namespaces etc.).

To add custom statement-handling code, insert this method override:

```
fn after_stmt_analysis(
    &self,
    analysis_data: &mut TastInfo,
    after_stmt_analysis_data: AfterStmtAnalysisData,
) {
    // your code goes here
}
```

## after_expr_analysis

This hook is called every time an expression is analyzed. Expressions include method calls, assignments, array access and more. Expressions can themselves contain other expressions — for example, the left-hand-side of a propery fetch expression is itself an expression (normally a variable-fetch expression).

To add custom expression-handling code, insert this method override:

```
fn after_expr_analysis(
    &self,
    analysis_data: &mut TastInfo,
    after_expr_analysis_data: AfterExprAnalysisData,
) {
    // your code goes here
}
```

## handle_functionlike_param

This hook is run when analysing a function or method’s parameters, before Hakana analyzes the function body.

You can use this hook to insert type-aware replacements for function parameters.

```
fn handle_functionlike_param(
    &self,
    analysis_data: &mut TastInfo,
    functionlike_param_data: FunctionLikeParamData,
) {
    // your code goes here
}
```

## after_argument_analysis

This hook is run after analysing every argument in a given function of method call.

If you want to change the type of a given function or method’s parameters, you normally also want to update its callers.

This hook allows you to do that on a per-call basis.

```
fn after_argument_analysis(
    &self,
    analysis_data: &mut TastInfo,
    after_arg_analysis_data: AfterArgAnalysisData,
) {
    // your code goes here
}
```
