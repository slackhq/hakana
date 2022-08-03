class A {}
class B extends A {}
class C extends B {}

function bad(A $x) : void {
    if (($x is C && rand(0, 1)) || rand(0, 1)) {
        return;
    }

    if ($x is B) {
        if ($x is C) {}
    }
}