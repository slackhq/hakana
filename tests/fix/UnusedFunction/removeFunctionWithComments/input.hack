function foo(): void {
    bar();
}

// this is a comment
/*
 * this is also a comment that spans
 * many lines
 */
function baz(): void {
    // do nothing
}

function bar(): void {
    foo();
}