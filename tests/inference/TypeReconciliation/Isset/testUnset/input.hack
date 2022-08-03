$foo = vec["a", "b", "c"];
foreach ($foo as $bar) {}
unset($foo, $bar);

function foo(): void {
    $foo = vec["a", "b", "c"];
    foreach ($foo as $bar) {}
    unset($foo, $bar);
}