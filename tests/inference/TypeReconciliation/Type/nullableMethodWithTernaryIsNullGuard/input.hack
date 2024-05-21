final class A {
    public function fooFoo(): void {}
}

final class B {
    public function barBar(?A $a = null) : void {
        $b = $a is null ? null : $a->fooFoo();
    }
}