namespace Name\Space {
    final class A {

    }

    final class B {

    }
}

namespace Noom\Spice {
    use Name\Space\{
        A,
        B
    };

    new A();
    new B();
}