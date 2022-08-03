class A {
    public function fooFoo(int $a): void {}
}

class B {
    public function fooFoo(int $a, string $s): void {}
}

(rand(0, 1) ? new A : new B)->fooFoo(5, "dfd");