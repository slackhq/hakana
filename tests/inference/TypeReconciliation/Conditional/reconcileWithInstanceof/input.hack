abstract class A {}
final class B extends A {
    public function b() : bool {
        return (bool) rand(0, 1);
    }
}

function bar(?A $a) : void {
    if (!$a || ($a is B && $a->b())) {}
}