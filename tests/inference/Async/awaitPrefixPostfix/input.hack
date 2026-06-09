<<file:__EnableUnstableFeatures('allow_extended_await_syntax', 'allow_conditional_await_syntax')>>

async function get_index(): Awaitable<int> {
    await \HH\Asio\usleep(1);
    return 0;
}

async function prefix_increment(): Awaitable<vec<int>> {
    $x = vec[1, 2, 3];
    ++$x[await get_index()];
    return $x;
}

async function postfix_increment(): Awaitable<vec<int>> {
    $x = vec[1, 2, 3];
    $x[await get_index()]++;
    return $x;
}
