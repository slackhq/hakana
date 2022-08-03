trait T {
    public function fooFoo(): void {
    }
}

class B {
    use T;

    public function fooFoo(string $a): void {
    }
}

(new B)->fooFoo("hello");