function foo(string $t, bool $b) : void {
    if ($t !== "a") {
        if ($t === "b" && $b) {}
    }
}

function bar(string $t, bool $b) : void {
    if ($t !== "a") {
        if ($t === "b" || $b) {}
    }
}