function foo(string $t, bool $b) : void {
    if (!$b && $t === "a") {
        return;
    }

    if ($t === "c") {
        if (!$b && bar($t)) {}
    }
}

function bar(string $b) : bool {
    return true;
}