class A {
    public function fooFoo(): void { }
}
function bar (vec_or_dict $a): void {
    if ($a["a"] is A) {
        $a["a"]->fooFoo();
    }
}