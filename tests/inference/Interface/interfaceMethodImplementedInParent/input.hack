interface MyInterface {
    public function fooFoo(int $a): void;
}

class B {
    public function fooFoo(int $a): void {

    }
}

class C extends B implements MyInterface { }