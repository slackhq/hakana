abstract class A {
    public function __construct(protected int $foo) {}
}

final class AChild extends A {
    public function bar() : int {
        return $this->foo;
    }
}