<<file:__EnableUnstableFeatures('allow_extended_await_syntax', 'allow_conditional_await_syntax')>>

async function should_repeat(): Awaitable<bool> {
    await \HH\Asio\usleep(1);
    return false;
}

async function do_while_with_await(): Awaitable<int> {
    $count = 0;
    do {
        $count++;
    } while (await should_repeat());
    return $count;
}
