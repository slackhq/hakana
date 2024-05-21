trait Foo {
    final public function foo(): this
    {
        return $this;
    }
}

final class A {
    use Foo;
}