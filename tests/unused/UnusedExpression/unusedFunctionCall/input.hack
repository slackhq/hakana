<<Hakana\MustUse>>
function must_use(): int {
    return 0;
}

function foo(): void {
    must_use();
}
