function foo<T>(T $one, (function():(T, T)) $two): T {
    if (rand(0, 1)) {
        return $one;
    }
    return $two()[0];
}

function bar(): void {
    $a = foo(1, () ==> tuple("three", "four"));

    if ($a is int) {
        // do something
    }

    if ($a is string) {
        // do something else
    }
}