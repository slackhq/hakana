function foo($a) : void {
    $b = $a is nonnull ? 1 : 0;
    $c = $a is nonnull ? 1 : 0;
}