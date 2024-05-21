trait T {
    protected function fooFoo(): void {
    }
}

abstract class B {
    use T;
}

final class C extends B {
    public function doFoo(): void {
        $this->fooFoo();
    }
}