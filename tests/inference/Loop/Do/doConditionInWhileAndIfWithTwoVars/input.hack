function foo(bool $b): void {
    do {
        $s = rand(0, 1) !== 0;
    } while (!$b && $s);

    if ($b) {}
}

function bar(bool $b): void {
    do {
        $s = rand(0, 1) !== 0;
        if (!$b && $s) {
            // do something
        }
    } while (!$b && $s);

    if ($b) {}
}
