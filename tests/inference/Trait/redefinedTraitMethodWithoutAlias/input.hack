trait T {
    public function fooFoo(): void {
    }
}

final class B {
    use T;

    public function fooFoo(string $a): void {
    }
}

(new B())->fooFoo("hello");