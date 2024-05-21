final class A {
    public function fooFoo(): void {

    }
}

final class B {
    public function barBar(): void {

    }
}

$a = rand(0, 10) ? new A() : new B();

switch (get_class($a)) {
    case A::class:
        $a->barBar();
        break;
}