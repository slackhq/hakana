abstract class A {}
final class B extends A {}
final class C extends A {}

function takesA(A $a): void {}

function foo(?A $a, ?A $b, ?A $c): void {
    if ($a === null || ($b !== null && $c !== null)) {
        return;
    }

    takesA($a);
}