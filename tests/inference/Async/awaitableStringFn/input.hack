function foo((function(): Awaitable<string>) $fn): void {}

foo(async (): Awaitable<string> {
    return "foo";
});
foo(async () {
    return "foo";
});