function foo(inout string $s) {
    $s = $_GET["a"];
}

function bar(): void {
    $a = "";
    foo(inout $a);
    echo $a;
}