final class A {
    public function fooFoo(): void { }
}
function bar (dict<string, mixed> $a): void {
    /** HAKANA_FIXME[PossiblyUndefinedStringArrayOffset] */
    if ($a["a"] is A) {
        $a["a"]->fooFoo();
    }
}