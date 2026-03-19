async function do_async_work(): Awaitable<int> {
    return 42;
}

function get_data(): int {
    $result = \HH\Asio\join(do_async_work());
    return $result + 1;
}

async function async_caller(): Awaitable<int> {
    $x = await do_async_work();
    return get_data() + $x;
}

function sync_caller(): int {
    return get_data();
}
