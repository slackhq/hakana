interface A {
    public function fooFoo(int $a): void;
}

final class B implements A {
    <<__Override>>
    public function fooFoo(int $a): void {

    }
}