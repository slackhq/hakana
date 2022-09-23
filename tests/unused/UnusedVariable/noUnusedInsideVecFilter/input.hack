function foo(vec<int> $vec): vec<int> {
    return await HH\Lib\Vec\filter_async($vec, async $v ==> {
        $b = $v % 2 === 0;
        return $b;
    });
}