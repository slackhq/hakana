async function foo(): Awaitable<void> {
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
    await \HH\Asio\usleep(100000);
    echo $str;
}