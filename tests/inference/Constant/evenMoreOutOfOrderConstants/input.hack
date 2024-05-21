abstract class A {
    const X = self::Y;
    const Y = 3;
}

final class C extends B {
}

const Z = C::X;

abstract class B extends A {
    const Z = self::X;
}