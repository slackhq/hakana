function foo1(shape('id' => string, 'name' => string) $d): void {}
function foo2(shape('id' => string, 'name' => string, ...) $d): void {}

function from_mixed_with_check(mixed $m): void {
    if ($m is dict<_, _>) {
        foo1($m);
        foo2($m);
    }
}

function from_dict_mixed_with_check(dict<string, mixed> $d): void {
    foo1($d);
    foo2($d);
}