async function fetch_data_async(): Awaitable<int> {
    return 42;
}

// This sync function just wraps the async version
function fetch_data(): int {
    return Asio\join(fetch_data_async());
}

function caller(): int {
    return fetch_data(); // This should be fixed to Asio\join(fetch_data_async())
}

async function async_caller(): Awaitable<int> {
    return fetch_data(); // This should be fixed to await fetch_data_async()
}