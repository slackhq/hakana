function foo(inout dict<string, mixed> $arr): void {
    $arr['a'] = 5;
}

<<__EntryPoint>>
function bar(): void {
    $barr = dict[];
    foo(inout $barr);
}