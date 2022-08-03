class A {}
class B extends A {}
class C extends A {}

function takesA(A $a): void {}

function foo(?A $a, ?A $b, ?A $c): void {
    if (!$a || ($b && $c)) {
        return;
    }

    takesA($a);
}