interface A {}
interface B {}

final class C implements A, B {}

function takesB(B $b): void {}

function foo(A $i): A {
    if ($i is B) {
        takesB($i);
        return $i;
    }
    return $i;
}

foo(new C());