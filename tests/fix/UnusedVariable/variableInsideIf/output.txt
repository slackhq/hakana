function foo(): void {
    if (rand(0, 1))
        $a = null;
    if (rand(0, 1)) $b = null;
    $b = 0;
    echo $b;
}
