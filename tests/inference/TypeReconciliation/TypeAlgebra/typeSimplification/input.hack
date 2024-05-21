abstract class A {}
final class B extends A {}

function foo(A $a, A $b) : ?B {
    if (($a is B || !$b is B) && $a is B && $b is B) {
        return $a;
    }

    return null;
}