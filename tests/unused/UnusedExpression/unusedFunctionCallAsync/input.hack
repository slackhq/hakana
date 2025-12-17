<<Hakana\MustUse>>
async function must_use_async(): Awaitable<int> {
    await \HH\Asio\usleep(100000);
    return 0;
}

function foo(): void {
    Asio\join(must_use_async());
}

async function foo_async(): Awaitable<void> {
    await must_use_async();
}
