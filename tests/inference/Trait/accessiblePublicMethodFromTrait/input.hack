trait T {
    public function fooFoo(): void {
    }
}

class B {
    use T;

    public function doFoo(): void {
        $this->fooFoo();
    }
}