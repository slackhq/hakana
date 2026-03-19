async function do_async_work(): Awaitable<int> {
    return 42;
}

<<__DynamicallyCallable>>
function get_data(): int {
    $result = \HH\Asio\join(do_async_work());
    return $result + 1;
}

async function caller(): Awaitable<int> {
    $x = await do_async_work();
    return get_data() + $x;
}
