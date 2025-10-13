trait T {
    public function fooFoo(): void {
    }
}

final class B {
    use T;

    <<__Override>>
    public function fooFoo(string $a): void {
    }
}

(new B())->fooFoo("hello");