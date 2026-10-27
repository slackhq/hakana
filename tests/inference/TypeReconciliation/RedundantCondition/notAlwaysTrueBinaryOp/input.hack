function foo ($a) : void {
    if ($a is null) {}
    $b = $a is nonnull && rand(0, 1) !== 0;
}