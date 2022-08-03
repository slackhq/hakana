trait T {
    public function fooFoo(): void {
    }
}

class B {
    use T;
}

class C extends B {
    public function doFoo(): void {
        $this->fooFoo();
    }
}