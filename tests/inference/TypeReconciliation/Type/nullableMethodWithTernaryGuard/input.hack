class A {
    public function fooFoo(): void {}
}

class B {
    public function barBar(?A $a = null) : void {
        $b = $a ? $a->fooFoo() : null;
    }
}