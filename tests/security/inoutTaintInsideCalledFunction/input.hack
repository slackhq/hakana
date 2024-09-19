function foo(inout string $s) {
    $s = HH\global_get('_GET')["a"];
}

function bar(): void {
    $a = "";
    foo(inout $a);
    echo $a;
}