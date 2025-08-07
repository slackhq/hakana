async function inner_async(): Awaitable<Awaitable<string>> {
    return async { return "nested result"; };
}

async function test_nested_join(): Awaitable<void> {
    $inner = HH\Asio\join(inner_async());
    $result = HH\Asio\join($inner);
    echo $result;
}

async function test_join_in_condition(): Awaitable<void> {
    if (HH\Asio\join(inner_async()) !== null) {
        echo "Not null";
    }
}

// Test join is not converted in non-async context
function non_async_function(): void {
    // This should not be converted since it's not inside an async function
    $result = HH\Asio\join(inner_async());
}