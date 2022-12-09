class A {}

function foo(shape('a' => string) $s): void {
    if ($s is A) {}
}