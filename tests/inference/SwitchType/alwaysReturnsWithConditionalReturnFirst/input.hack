function getRows(string $s) : int {
    if (rand(0, 1)) {
        return 1;
    }

    switch ($s) {
        case "a":
            return 2;
        default:
            return 1;
    }
}