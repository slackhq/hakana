final class A {
    public function fooFoo(): void {}
}

final class B {
    public ?A $a = null;

    public function barBar(?A $a = null) : void {
        $this->a = $a;

        if ($this->a) {
            $this->a->fooFoo();
        }
    }
}