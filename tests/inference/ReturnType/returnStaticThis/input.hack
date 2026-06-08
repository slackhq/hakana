namespace Foo;

abstract class A {
    public function getThis() : this {
        return $this;
    }

    public static function create(): this {
        return self::createImpl();
    }

    private static function createImpl(): this {
        return new static();
    }
}

final class B extends A {
    public function foo() : void {}
}

(new B())->getThis()->foo();
