<<file:__EnableUnstableFeatures('allow_extended_await_syntax', 'allow_conditional_await_syntax')>>

async function returns_bool(): Awaitable<bool> {
    await \HH\Asio\usleep(1);
    return true;
}

async function short_circuit_or(): Awaitable<bool> {
    return await returns_bool() || await returns_bool();
}

async function short_circuit_and(): Awaitable<bool> {
    return await returns_bool() && await returns_bool();
}
