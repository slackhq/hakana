async function foo(): Awaitable<string> {
    return $_GET["evil"];
}

function bar(): void {
    echo HH\Asio\join(foo());
}