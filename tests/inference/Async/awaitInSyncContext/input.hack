// This should error - await in sync function
function syncFunc(): void {
    $x = await someAsyncFunc();
}

async function someAsyncFunc(): Awaitable<int> {
    await HH\Asio\usleep(1);
    return 1;
}

// This is fine - await in async function
async function asyncFunc(): Awaitable<void> {
    $x = await someAsyncFunc();
}
