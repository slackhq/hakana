<<file:__EnableUnstableFeatures('allow_extended_await_syntax', 'allow_conditional_await_syntax')>>

async function should_continue(): Awaitable<bool> {
    await \HH\Asio\usleep(1);
    return false;
}

async function while_with_await(): Awaitable<int> {
    $count = 0;
    while (await should_continue()) {
        $count++;
    }
    return $count;
}
