function foo(string $s) : void {
    switch ($s) {
        case "a":
        case "b":
        case "c":
            if (in_array($s, vec["a", "b"], true)) {
                throw new \InvalidArgumentException();
            }
            break;
    }
}