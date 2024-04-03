<<__EntryPoint, Hakana\TestOnly>>
function main(): void {
    test_fn();
    other_fn();
}

<<Hakana\TestOnly>>
function test_fn(): void {}

<<Hakana\TestOnly>>
function other_fn(): void {
    test_fn();
}