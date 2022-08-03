namespace NS {
    use OtherNS\C as E;

    class A {}

    class C extends A {}

    class D extends A {};

    function foo(classname<A> $s) : void {}

    foo(C::class);

    foo(D::class);

    foo(E::class);

    foo(\OtherNS\C::class);
}

namespace OtherNS {
    class C extends \NS\A {}
}