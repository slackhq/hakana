
function bar (string $name): int {
    switch ($name) {
            case "a":
            case ucfirst("a"):
                return 1;
    }
    return -1;
}