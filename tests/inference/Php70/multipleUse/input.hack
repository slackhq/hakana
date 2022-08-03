namespace Name\Space {
    class A {

    }

    class B {

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