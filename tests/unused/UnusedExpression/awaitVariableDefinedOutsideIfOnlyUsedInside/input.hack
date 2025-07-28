async function foo(): Awaitable<string> {
    await \HH\Asio\usleep(100000);
    return "hello";
}

async function bar(): Awaitable<void> {
    $result = await foo();
    if (rand(0, 1)) {
        echo $result;
    }
}