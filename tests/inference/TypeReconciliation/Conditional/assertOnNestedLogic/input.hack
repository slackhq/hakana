function foo(?string $a) : void {
    if (($a is nonnull && rand(0, 1) !== 0) || rand(0, 1) !== 0) {
        if ($a is nonnull && HH\Lib\Str\length($a) > 5) {}
    }
}
