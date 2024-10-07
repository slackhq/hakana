final class A<T> {
    public function __construct(public T $t) {}
}

function makeA<T>(T $t): A<T> {
    return new A($t);
}

enum B: int as int {
    A = 1;
    B = 2;
    C = 3;
}

function foo(): void {
    $a = makeA(B::B);
    bar($a);
}

function bar(A<B> $_a): void {
}