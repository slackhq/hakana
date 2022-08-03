class Foo {
    public function __construct(UndefinedClass $o) {}
}
new Foo(function() : void {});