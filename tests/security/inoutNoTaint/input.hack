function foo(inout string $s) {
    // do nothing
}

function bar(): void {
    $a = $_GET["a"];
    foo(inout $a);

    $b = "hello";
    foo(inout $b);
    echo $b;
}