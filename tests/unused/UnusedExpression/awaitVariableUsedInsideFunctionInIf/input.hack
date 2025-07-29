async function foo(): Awaitable<keyset<int>> {
    await \HH\Asio\usleep(100000);
    return keyset[1, 2, 3];
}

async function bar(): Awaitable<void> {
    $result = await foo();
    if (rand(0, 1)) {
        takesKeyset($result);
    }
}

function takesKeyset(keyset<int> $_k): void {}