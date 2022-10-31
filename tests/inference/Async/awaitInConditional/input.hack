async function foo(?string $s): Awaitable<string> {
    if ($s is null) {
        if (await bar() == 4) {
            return '';
        }
        return '1';
    }
    
    return $s;
}

async function bar(): Awaitable<int> {
    return 5;
}