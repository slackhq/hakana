<<__EntryPoint>>
function main(): void {
    test_fn();
    other_fn();
}

<<Hakana\TestOnly>>
function test_fn(): void {}

function other_fn(): void {
    test_fn();
}