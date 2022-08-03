namespace {
    class Deprecated implements HH\ClassAttribute {}
}

namespace Foo\Bar {
    function foo(<<\Deprecated>> string $foo) : void {}
}