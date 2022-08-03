class A {}
class B extends A {}
class C extends A {}

function takesA(A $a): void {}

function foo(?A $a): void {
    if (($a is B && rand(0, 1))
        || ($a is C && rand(0, 1))
    ) {
        takesA($a);
    }
}