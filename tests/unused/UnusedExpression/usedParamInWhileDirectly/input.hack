function foo(int $index): void {
    while (100 >= $index = nextNumber($index)) {
        // ...
    }
}

function nextNumber(int $eee): int {
    return $eee + 1;
}