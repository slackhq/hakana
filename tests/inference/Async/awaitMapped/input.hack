async function foo(vec<int> $vec): Awaitable<int> {
    $vec = await HH\Lib\Vec\map_async(
        $vec,
        $i ==> bar($i),
    );

    if (!C\is_empty($vec)) {
        return $vec[0];
    }

    return 0;
}

async function bar(int $i): Awaitable<int> {
    await \HH\Asio\usleep(100000);
    return $i + 5;
}