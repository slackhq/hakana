async function foo(?string $s): Awaitable<shape('a' => arraykey)> {
    if ($s is null) {
        return shape('a' => await bar());
    }
    
    return shape('a' => $s);
}

async function bar(): Awaitable<int> {
    return 5;
}