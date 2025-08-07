async function simple_async(): Awaitable<string> {
    return "hello";
}

async function test_simple_join(): Awaitable<void> {
    $result = HH\Asio\join(simple_async());
    echo $result;
}

async function test_multiple_joins(): Awaitable<void> {
    $result1 = HH\Asio\join(simple_async());
    $result2 = HH\Asio\join(simple_async());
    echo $result1 . $result2;
}