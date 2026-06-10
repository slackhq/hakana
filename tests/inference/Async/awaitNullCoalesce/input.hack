<<file:__EnableUnstableFeatures('allow_extended_await_syntax', 'allow_conditional_await_syntax')>>

async function get_nullable_string(): Awaitable<?string> {
    await \HH\Asio\usleep(1);
    return null;
}

async function get_string(): Awaitable<string> {
    await \HH\Asio\usleep(1);
    return "default";
}

async function null_coalesce_await(): Awaitable<string> {
    return await get_nullable_string() ?? await get_string();
}
