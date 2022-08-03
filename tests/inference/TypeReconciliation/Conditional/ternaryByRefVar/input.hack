function foo(): void {
    $b = null;
    $c = rand(0, 1) ? bar($b) : null;
    if ($b is int) { }
}
function bar(inout ?int $a): void {
    $a = 5;
}