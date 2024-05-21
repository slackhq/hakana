async function foo(vec<Awaitable<void>> $vec): Awaitable<void> {
    await HH\Lib\Vec\from_async($vec);
}