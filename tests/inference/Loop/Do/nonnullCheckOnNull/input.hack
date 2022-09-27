function foo(): void {
    $a = null;
    do {
        if ($a is nonnull) {
            $b = $a;
        }

        $a = rand(0, 1) ? 'hello' : null;
    } while (rand(0, 1));
}