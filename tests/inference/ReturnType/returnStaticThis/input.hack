namespace Foo;

class A {
    public function getThis() : this {
        return $this;
    }
}

class B extends A {
    public function foo() : void {}
}

(new B())->getThis()->foo();