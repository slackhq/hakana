function foo(): void {
    if (rand(0, 1) !== 0)
        $a = null;
    if (rand(0, 1) !== 0) $b = null;
    $b = 0;
    echo $b;
}
