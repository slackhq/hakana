interface A {}
interface B {}

class C implements A, B {}

function foo(A $i): B {
    if ($i is B) {
        return $i;
    }
    throw new \Exception("bad");
}

foo(new C());