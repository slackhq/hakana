function foo(): int {
    $a = rand(0, 1) ? 3 : false;

    if ($a !== false && rand(0, 1)) {
        $a = rand(0, 1) ? 3 : false;
        if ($a !== false) {
            return $a;
        }
    }

    return 0;
}