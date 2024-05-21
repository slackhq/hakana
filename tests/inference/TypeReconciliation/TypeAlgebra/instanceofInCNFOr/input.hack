abstract class A {}
final class B extends A {}
final class C extends A {}

function takesA(A $a): void {}

function foo(?A $a): void {
    $c = rand(0, 1);
    if (($a is B || $a is C)
        && ($a is B || $c)
    ) {
        takesA($a);
    }
}