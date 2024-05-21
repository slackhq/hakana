abstract class A {
    protected function fooFoo(): void {
    }
}

final class B extends A { }

final class C extends A {
    public function doFoo(): void {
        (new B())->fooFoo();
    }
}