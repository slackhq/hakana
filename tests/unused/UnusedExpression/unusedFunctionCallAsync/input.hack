<<Hakana\MustUse>>
async function must_use_async(): Awaitable<int> {
    return 0;
}

function foo(): void {
    Asio\join(must_use_async());
}

function foo_async(): void {
    await must_use_async();
}
