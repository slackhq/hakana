class A {
    public function fooFoo(): void {}
}

class B {
    public function barBar(?A $a = null) : void {
        if ($a) {
            $a->fooFoo();
        }
    }
}