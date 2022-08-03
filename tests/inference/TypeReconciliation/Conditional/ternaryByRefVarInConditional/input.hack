function foo(): void {
    $b = null;
    if (rand(0, 1) || bar(inout $b)) {
        if ($b is int) { }
    }
}
function bar(inout ?int $a): void {
    $a = 5;
}