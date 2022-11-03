function foo($a, arraykey $b): void {
    if ($a) {
        if (rand(0, 1)) {
            $a = $b;
        }
    }

    if ($a) {}
}