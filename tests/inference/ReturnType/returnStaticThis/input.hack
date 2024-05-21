namespace Foo;

abstract class A {
    public function getThis() : this {
        return $this;
    }
}

final class B extends A {
    public function foo() : void {}
}

(new B())->getThis()->foo();