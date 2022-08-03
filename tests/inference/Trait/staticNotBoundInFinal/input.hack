trait Foo {
    final public function foo(): this
    {
        return $this;
    }
}

class A {
    use Foo;
}