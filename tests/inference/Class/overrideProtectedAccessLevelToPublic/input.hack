abstract class A {
    protected function fooFoo(): void {}
}

final class B extends A {
    <<__Override>>
    public function fooFoo(): void {}
}