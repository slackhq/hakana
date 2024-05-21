final class A {
    public function __construct(public int $foo = 5) {}
}

echo (new A())->foo;