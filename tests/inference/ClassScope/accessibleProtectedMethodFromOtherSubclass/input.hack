class A {
    protected function fooFoo(): void {
    }
}

class B extends A { }

class C extends A {
    public function doFoo(): void {
        (new B)->fooFoo();
    }
}