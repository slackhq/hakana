async function get_number(): Awaitable<int> {
    return 42;
}

async function test_join_in_expression(): Awaitable<void> {
    $result = HH\Asio\join(get_number()) + 10;
    echo $result;
}

async function test_join_in_function_call(): Awaitable<void> {
    echo intval(HH\Asio\join(get_number()));
}

async function test_join_in_concat(): Awaitable<void> {
    echo "Result: " . HH\Asio\join(get_number());
}