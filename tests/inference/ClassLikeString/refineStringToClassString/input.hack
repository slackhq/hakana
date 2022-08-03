class A {}

function foo(string $s) : ?A {
    if ($s !== A::class) {
        return null;
    }
    return new $s();
}