interface MyInterface {
    public function fooFoo(int $a): void;
}

abstract class B {
    public function fooFoo(int $a): void {

    }
}

final class C extends B implements MyInterface { }