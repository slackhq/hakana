abstract class A {
    protected function fooFoo(): void {
    }
}

final class B extends A {
    public function doFoo(): void {
        $this->fooFoo();
    }
}