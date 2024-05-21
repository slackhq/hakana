abstract class A {
    const A = 1;

    public static function test(): void {
        echo B::A;
    }
}

final class B extends A {
    const A = 2;
}

A::test();