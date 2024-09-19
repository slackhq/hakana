async function foo(): Awaitable<string> {
    return HH\global_get('_GET')["evil"];
}

function bar(): void {
    echo HH\Asio\join(foo());
}

async function baz(): Awaitable<void> {
    echo await foo();
}