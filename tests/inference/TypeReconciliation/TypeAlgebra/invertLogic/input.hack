abstract class A {}
final class B extends A {}

function foo(?A $a) : A {
    if (!$a || !($a is B && rand(0, 1))) {
        throw new Exception();
    }

    return $a;
}