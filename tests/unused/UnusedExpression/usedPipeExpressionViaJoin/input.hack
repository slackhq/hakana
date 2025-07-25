async function foo(): Awaitable<void> {
    await \HH\Asio\usleep(100000);
    echo "hello";
}

function bar(): string {
    foo() |> HH\Asio\join($$);
}