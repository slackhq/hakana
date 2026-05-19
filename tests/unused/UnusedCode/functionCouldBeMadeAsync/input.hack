async function do_async_work(): Awaitable<int> {
    return 42;
}

function get_data(): int {
    $result = \HH\Asio\join(do_async_work());
    return $result + 1;
}

async function caller1(): Awaitable<int> {
    $x = await do_async_work();
    return get_data() + $x;
}

async function caller2(): Awaitable<void> {
    $x = await do_async_work();
    echo get_data() + $x;
}
