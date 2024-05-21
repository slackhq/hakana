function foo()[]: Awaitable<vec<string>> {
    return async { return vec["a"]; };
}

async function bar()[]: Awaitable<vec<string>> {
    return vec["a"];
}

async function baz(): Awaitable<vec<string>> {
    echo "a";
    return vec["a"];
}

function main(): void {
    $a = foo() |> HH\Asio\join($$);
    $b = foo() |> vec[$$];
    $c = foo() |> HH\Asio\join($$) |> C\first($$);
    $a = bar() |> HH\Asio\join($$);
    $b = bar() |> vec[$$];
    $c = bar() |> HH\Asio\join($$) |> C\first($$);
    $a = baz() |> HH\Asio\join($$);
    $b = baz() |> vec[$$];
    $c = baz() |> HH\Asio\join($$) |> C\first($$);
}
