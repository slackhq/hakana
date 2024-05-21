final class A {
    public function __construct(public int $foo) {}
}

echo (new A(5))->foo;