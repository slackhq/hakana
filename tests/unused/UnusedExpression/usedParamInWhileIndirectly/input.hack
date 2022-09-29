function foo(int $i): void {
    $index = $i;
    while (100 >= $index = nextNumber($index)) {
        // ...
    }
}

function nextNumber(int $i): int {
    return $i + 1;
}