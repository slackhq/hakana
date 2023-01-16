function foo(string $s) : void {
    switch ($s) {
        case "a":
        case "b":
        case "c":
            if ($s === "a" || $s === "b") {
                throw new \InvalidArgumentException();
            }
            break;
    }
}