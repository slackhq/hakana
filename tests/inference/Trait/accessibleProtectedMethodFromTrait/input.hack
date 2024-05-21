trait T {
    protected function fooFoo(): void {
    }
}

final class B {
    use T;

    public function doFoo(): void {
        $this->fooFoo();
    }
}