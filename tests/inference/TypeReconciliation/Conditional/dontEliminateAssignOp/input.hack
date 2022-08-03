class Obj {}
class A extends Obj {}
class B extends A {}
class C extends Obj {}
class D extends C {}
class E extends C {}

function bar(Obj $node) : void {
    if ($node is B
        || $node is D
        || $node is E
    ) {
        if ($node is C) {}
        if ($node is D) {}
    }
}