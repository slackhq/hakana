final class A {
    public static function bar(): int {
        return 5;
    }
}

function foo(A $foo): int {
    return $foo::bar();
}
