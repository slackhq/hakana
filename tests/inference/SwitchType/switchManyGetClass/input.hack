abstract class A {}
final class B extends A {}
final class C extends A {}
final class D extends A {}

function foo(A $a) : void {
    switch(get_class($a)) {
        case B::class:
        case C::class:
        case D::class:
            echo "goodbye";
    }
}