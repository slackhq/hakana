namespace FunctionNamespace {
    function foo() : void {}
}

namespace ClassNamespace {
    final class Foo {}
}

namespace {
    use ClassNamespace\Foo;
    use function FunctionNamespace\foo;

    new Foo();

    foo();
}