class A {
    const X = self::Y;
    const Y = 3;
}

class C extends B {
}

const Z = C::X;

class B extends A {
    const Z = self::X;
}