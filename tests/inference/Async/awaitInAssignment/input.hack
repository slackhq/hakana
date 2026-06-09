<<file:__EnableUnstableFeatures('allow_extended_await_syntax', 'allow_conditional_await_syntax')>>

async function get_index(): Awaitable<int> {
    await \HH\Asio\usleep(1);
    return 0;
}

async function get_value(): Awaitable<string> {
    await \HH\Asio\usleep(1);
    return "hello";
}

async function assignment_with_await(): Awaitable<vec<string>> {
    $x = vec['a', 'b', 'c'];
    $x[await get_index()] = await get_value();
    return $x;
}
