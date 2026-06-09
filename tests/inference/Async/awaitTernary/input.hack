<<file:__EnableUnstableFeatures('allow_extended_await_syntax', 'allow_conditional_await_syntax')>>

async function get_bool(): Awaitable<bool> {
    await \HH\Asio\usleep(1);
    return true;
}

async function get_int(): Awaitable<int> {
    await \HH\Asio\usleep(1);
    return 42;
}

async function ternary_await(): Awaitable<int> {
    return await get_bool() ? await get_int() : await get_int();
}
