enum Foo: int {
    BAR = 1;
    BAZ = 2;
}

function bar(Foo $a, Foo $b): int {
    return ($a as int) + ($b as int);
}