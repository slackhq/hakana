class A {}

class B extends A {}

class C {}

/* HHAST_FIXME[FinalOrAbtractClass] */
class D {}

<<__EntryPoint>>
function main(): void {
    new B();
    new C();
    new D();
}