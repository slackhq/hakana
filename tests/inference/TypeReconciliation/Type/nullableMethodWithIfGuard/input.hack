final class A {
    public function fooFoo(): void {}
}

final class B {
    public function barBar(?A $a = null) : void {
        if ($a !== null) {
            $a->fooFoo();
        }
    }
}