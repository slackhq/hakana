function foo(): void {
    $b = null;
    if (rand(0, 1) !== 0 || bar(inout $b)) {
        if ($b is int) { }
    }
}
function bar(inout ?int $a): bool {
    $a = 5;
    return true;
}