function foo(string $t, bool $b) : void {
    if ($t === "a") {
    } else if ($t === "b" && $b) {}
}

function bar(string $t, bool $b) : void {
    if ($t === "a") {
    } else if ($t === "b" || $b) {}
}