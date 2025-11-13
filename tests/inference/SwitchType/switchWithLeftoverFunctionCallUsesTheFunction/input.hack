
function bar (string $name): int {
    switch ($name) {
            case "a":
            case HH\Lib\Str\capitalize("a"):
                return 1;
    }
    return -1;
}