function foo(inout string $s) {
    // do nothing
}

function bar(): void {
    $a = $_GET["a"];
    foo(inout $a);
    echo $a;
}