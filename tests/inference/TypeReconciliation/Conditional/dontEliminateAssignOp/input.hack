abstract class Obj {}
abstract class A extends Obj {}
final class B extends A {}
abstract class C extends Obj {}
final class D extends C {}
final class E extends C {}

function bar(Obj $node) : void {
    if ($node is B
        || $node is D
        || $node is E
    ) {
        if ($node is C) {}
        if ($node is D) {}
    }
}