function foo(): void {
    await Vec\map_async(
        vec["hello", "world"],
        async $item ==> myFunc($item)
    );

    await Vec\map_async(
        vec["hello", "world"],
        async $item ==> await myFunc($item)
    );
}

async function myFunc(string $str): Awaitable<void> {
    echo $str;
}