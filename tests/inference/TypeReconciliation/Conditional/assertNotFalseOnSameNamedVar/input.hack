function foo(): int {
    $a = rand(0, 1) !== 0 ? 3 : false;

    if ($a !== false && rand(0, 1) !== 0) {
        $a = rand(0, 1) !== 0 ? 3 : false;
        if ($a !== false) {
            return $a;
        }
    }

    return 0;
}
