abstract class A {
    public function foo() : void {}
}

trait T {
    abstract public function foo() : void;
}

final class B extends A {
    use T;
}