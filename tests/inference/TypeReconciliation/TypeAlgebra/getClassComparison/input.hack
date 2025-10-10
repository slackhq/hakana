abstract class Foo {
    public function bar() : void {}
}
final class Bar extends Foo{
    <<__Override>>
    public function bar() : void {}
}

final class Baz {
    public function test(Foo $foo) : void {
        if (get_class($foo) !== Foo::class) {
            // do nothing
        } else {
            $foo->bar();
        }
    }
}