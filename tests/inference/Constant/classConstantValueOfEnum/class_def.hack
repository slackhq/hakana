enum Foo: int as int {
    BAR = 1;
    BAZ = 2;
}

final class A {
    const int FOO = Foo::BAR;
}