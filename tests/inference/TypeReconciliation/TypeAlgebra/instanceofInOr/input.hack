class A {}
class B extends A {}
class C extends A {}

function takesA(A $a): void {}

function foo(?A $a): void {
    if ($a is B
        || ($a is C && rand(0, 1))
    ) {
        takesA($a);
    }
}