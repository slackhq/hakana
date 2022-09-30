class A {
    public function fooFoo(): void { }
}
function bar (dict<string, mixed> $a): void {
    if ($a["a"] is A) {
        $a["a"]->fooFoo();
    }
}