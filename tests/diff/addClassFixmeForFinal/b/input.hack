abstract class A {}

final class B extends A {}

/* HHAST_FIXME[FinalOrAbstractClass] */
class C {}

/* HHAST_FIXME[FinalOrAbstractClass] */
class D {}

<<__EntryPoint>>
function main(): void {
    new B();
    new C();
    new D();
}