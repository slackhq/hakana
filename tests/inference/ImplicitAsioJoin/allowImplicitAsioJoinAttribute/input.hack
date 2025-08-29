async function get_external_data(): Awaitable<int> {
    return 42;
}

async function fetch_data_async(): Awaitable<int> {
    $data = await get_external_data();
    return $data;
}

// This sync function wraps the async version but uses the attribute to prevent ImplicitAsioJoin error
<<Hakana\AllowImplicitAsioJoin>>
function fetch_data(): int {
    return Asio\join(fetch_data_async());
}

function caller(): int {
    return fetch_data(); // This should NOT trigger ImplicitAsioJoin due to the attribute
}

async function async_caller(): Awaitable<int> {
    $result = await get_external_data();
    return fetch_data() + $result; // This should also NOT trigger ImplicitAsioJoin due to the attribute
}