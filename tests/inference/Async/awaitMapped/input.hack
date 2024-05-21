async function foo(vec<int> $vec): Awaitable<int> {
    $vec = await HH\Lib\Vec\map_async(
        $vec,
        $i ==> bar($i),
    );

    if ($vec) {
        return $vec[0];
    }

    return 0;
}

async function bar(int $i): Awaitable<int> {
    return $i + 5;
}