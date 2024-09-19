function foo(inout string $s) {
    // do nothing
}

function bar(): void {
    $a = HH\global_get('_GET')["a"];
    foo(inout $a);
    echo $a;
}