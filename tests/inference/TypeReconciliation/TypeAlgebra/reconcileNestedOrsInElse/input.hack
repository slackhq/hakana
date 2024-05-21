final class A {}
final class B {}

function takesA(A $a): void {}

function foo(?A $a, ?B $b): void {
    if ($a === null || $b === null || rand(0, 1)) {
        // do nothing
    } else {
        takesA($a);
    }
}