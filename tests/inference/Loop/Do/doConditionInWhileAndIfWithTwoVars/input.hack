function foo(bool $b) {
    do {
        $s = rand(0, 1);
    } while (!$b && $s);

    if ($b) {}
}

function bar(bool $b) {
    do {
        $s = rand(0, 1);
        if (!$b && $s) {
            // do something
        }
    } while (!$b && $s);

    if ($b) {}
}
