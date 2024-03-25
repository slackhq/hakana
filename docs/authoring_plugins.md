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

## Hooks

Hakana provides hooks that are called as it analyzes various elements of the codebase:

- `after_expr_analysis` for expressions like `foo()`, `$arr[$key]`, `1 + 2`
- `after_stmt_analysis` for statements like `return` and `throw`
- `after_argument_analysis` for each argument to a function or method call
- `after_def_analysis` for definition like classes, functions, constants, and type aliases
- `handle_functionlike_param` for the parameters of a function or method

A given line of code can trigger multiple hooks. For this code:

```hack
return foo($a + $b['x'], $c);
```

Hakana will call `after_stmt_analysis` for the entire return statement; it will call `after_expr_analysis` for the call to `foo(...)`, the array access `$b['x']`, and the addition operation `$a + $b['x']`; and it will call `after_argument_analysis` for each of the arguments to foo, `$a + $b['x']` and `$c`.

analysis_data.add_replacement((start_offset as u32, end_offset as u32), replacement);

A hook can make changes to the code via `FunctionAnalysisData::add_replacement`. These changes are applied after all analysis is complete. A hook can also modify `FunctionAnalysisData` to provide information for other hooks or `get_candidates` to consume. For example, `after_expr_analysis` could add more information to the data flow graph that helps to determine how to change code in an `after_argument_analysis` call.

There are currently five hook methods you can use:

## after_stmt_analysis

This hook is called every time a statement is analyzed. There are many different kinds of statement in Hack — `if` statements, `return` statements, expression statements and many more.

A statement can be most easily thought of as any element you can put a semicolon after without changing the behaviour of your code.

They are distinct from expressions and definitions (functions, classes, namespaces etc.).

To add custom statement-handling code, insert this method override:

```
fn after_stmt_analysis(
    &self,
    analysis_data: &mut FunctionAnalysisData,
    after_stmt_analysis_data: AfterStmtAnalysisData,
) {
    // your code goes here
}
```

## after_expr_analysis

This hook is called every time an expression is analyzed. Expressions include method calls, assignments, array access and more. Expressions can themselves contain other expressions — for example, the left-hand-side of a propery fetch expression is itself an expression (normally a variable-fetch expression).

To add custom expression-handling code, insert this method override:

```rs
fn after_expr_analysis(
    &self,
    analysis_data: &mut FunctionAnalysisData,
    after_expr_analysis_data: AfterExprAnalysisData,
) {
    // your code goes here
}
```

## handle_functionlike_param

This hook is run when analysing a function or method’s parameters, before Hakana analyzes the function body.

You can use this hook to insert type-aware replacements for function parameters.

```rs
fn handle_functionlike_param(
    &self,
    analysis_data: &mut FunctionAnalysisData,
    functionlike_param_data: FunctionLikeParamData,
) {
    // your code goes here
}
```

## after_argument_analysis

This hook is run after analysing all the arguments in a given function of method call. It is called once for each argument to the call.

If you want to change the type of a given function or method’s parameters, you normally also want to update its callers.

This hook allows you to do that on a per-call basis.

```rs
fn after_argument_analysis(
    &self,
    analysis_data: &mut FunctionAnalysisData,
    after_arg_analysis_data: AfterArgAnalysisData,
) {
    // your code goes here
}
```

## after_def_analysis

This hook is run for each top-level definition (class, function, type alias, etc.).

```rs
fn after_def_analysis(
    &self,
    analysis_data: &mut FunctionAnalysisData,
    analysis_result: &mut AnalysisResult,
    after_def_analysis_data: AfterDefAnalysisData,
) {
    // your code goes here
}
```
