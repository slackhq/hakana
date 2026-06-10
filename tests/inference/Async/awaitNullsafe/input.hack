<<file:__EnableUnstableFeatures('allow_extended_await_syntax', 'allow_conditional_await_syntax')>>

final class Bar {
    public async function doWork(string $s): Awaitable<string> {
        await \HH\Asio\usleep(1);
        return $s;
    }
}

async function get_arg(): Awaitable<string> {
    await \HH\Asio\usleep(1);
    return "hello";
}

async function nullsafe_with_await(?Bar $bar): Awaitable<?string> {
    return await $bar?->doWork(await get_arg());
}
