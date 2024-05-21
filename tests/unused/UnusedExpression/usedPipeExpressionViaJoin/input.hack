async function foo(): Awaitable<void> {
    echo "hello";
}

function bar(): string {
    foo() |> HH\Asio\join($$);
}