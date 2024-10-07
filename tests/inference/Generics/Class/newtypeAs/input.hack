final class A<T> {
    public function __construct(public T $t) {}
}

function foo(A<foo_t> $a): void {
    bar($a);
}

function bar(A<string> $_a): void {
}