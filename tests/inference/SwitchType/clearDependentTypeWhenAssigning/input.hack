abstract class A {}

final class AChild extends A {
    public function bar() : void {}
}

final class B {}

function foo(A $a) : void {
    $a_class = get_class($a);

    $a = new B();

    switch ($a_class) {
        case AChild::class:
            $a->bar();
    }
}