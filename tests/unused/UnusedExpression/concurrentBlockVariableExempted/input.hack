async function test_concurrent_block(): Awaitable<void> {
    concurrent {
        $foo = await f();
        $x = await get_data_async();
    }
    if ($foo) {
        echo $x;
    }
}

async function f(): Awaitable<bool> {
    await \HH\Asio\usleep(100000);
    return true;
}

async function get_data_async(): Awaitable<string> {
    await \HH\Asio\usleep(100000);
    return "data";
}