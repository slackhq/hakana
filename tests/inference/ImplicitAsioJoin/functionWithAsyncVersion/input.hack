async function get_external_data(): Awaitable<int> {
    return 42;
}

async function fetch_data_async(): Awaitable<int> {
    $data = await get_external_data();
    return $data;
}

// This sync function just wraps the async version
function fetch_data(): int {
    return Asio\join(fetch_data_async());
}

function caller(): int {
    return fetch_data(); // This should trigger ImplicitAsioJoin
}

async function async_caller(): Awaitable<int> {
    $result = await get_external_data();
    return fetch_data() + $result; // This should also trigger but suggest await instead of Asio\join
}