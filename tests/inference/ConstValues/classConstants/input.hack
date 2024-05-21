namespace NS {
    use OtherNS\C as E;

    abstract class A {}

    final class C extends A {}

    final class D extends A {};

    function foo(classname<A> $s) : void {}

    foo(C::class);

    foo(D::class);

    foo(E::class);

    foo(\OtherNS\C::class);
}

namespace OtherNS {
    final class C extends \NS\A {}
}