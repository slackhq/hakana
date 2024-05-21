final class A {
    public function fooFoo(): void {}
}

final class B {
    public ?A $a = null;

    public function barBar(?A $a = null) : void {
        $this->a = $a;
        $b = $this->a ? $this->a->fooFoo() : null;
    }
}