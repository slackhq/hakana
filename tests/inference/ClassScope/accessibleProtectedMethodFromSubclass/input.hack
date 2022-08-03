class A {
    protected function fooFoo(): void {
    }
}

class B extends A {
    public function doFoo(): void {
        $this->fooFoo();
    }
}