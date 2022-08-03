class A {
    public function fooFoo(): void { }
}
function bar (vec_or_dict $a): void {
    if ($a[0] is A) {
        $a[0]->fooFoo();
    }
}