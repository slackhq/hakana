class Foo {
    public function bar() : void {}
}
class Bar extends Foo{
    public function bar() : void {}
}

class Baz {
    public function test(Foo $foo) : void {
        if (get_class($foo) !== Foo::class) {
            // do nothing
        } else {
            $foo->bar();
        }
    }
}