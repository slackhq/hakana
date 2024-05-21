interface A {
    public function fooFoo(int $a, int $b): void;
}

trait T {
    public function fooFoo(int $a, int $b): void {
    }
}

final class B implements A {
    use T;
}