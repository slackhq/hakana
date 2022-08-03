function foo(string $type, bool $and) : void {
    if ($type === "a") {
    } else if ($type === "b" && $and) {
    } else {
        if ($type === "c" && $and) {}
    }
}