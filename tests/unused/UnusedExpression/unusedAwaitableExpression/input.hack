function foo(): void {
    $a = bar(5);
}

async function bar(int $i): Awaitable<int> {
    await \HH\Asio\usleep(100000);
    return $i;
}