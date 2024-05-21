namespace {
    <<Attribute(Attribute::TARGET_CLASS | Attribute::TARGET_FUNCTION | Attribute::TARGET_PARAMETER)>>
    final class Deprecated {}
}

namespace Foo\Bar {
    <<\Deprecated>>
    function foo() : void {}
}