namespace Bar;

function test(\Foo\my_keys_t $t): void {
    $x = $t['a'];  // Should be int
    $y = $t['b'];  // Should be string
    $z = $t['c'];  // Should error - key doesn't exist
}
