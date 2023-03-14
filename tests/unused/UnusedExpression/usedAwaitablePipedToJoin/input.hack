function foo(): Awaitable<vec<string>> {
    return async { return vec["a"]; };
}

function bar(): void {
    $a = foo() |> HH\Asio\join($$);
    $b = foo() |> vec[$$];
    $c = foo() |> HH\Asio\join($$) |> C\first($$);
}