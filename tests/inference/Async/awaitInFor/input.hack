<<file:__EnableUnstableFeatures('allow_extended_await_syntax', 'allow_conditional_await_syntax')>>

async function get_start(): Awaitable<int> {
    await \HH\Asio\usleep(1);
    return 0;
}

async function get_limit(): Awaitable<int> {
    await \HH\Asio\usleep(1);
    return 10;
}

async function get_step(): Awaitable<int> {
    await \HH\Asio\usleep(1);
    return 2;
}

async function for_with_await(): Awaitable<int> {
    $sum = 0;
    for ($a = await get_start(); $a < await get_limit(); $a += await get_step()) {
        $sum += $a;
    }
    return $sum;
}
