function foo(?string $a) : void {
    if (($a && rand(0, 1) !== 0) || rand(0, 1) !== 0) {
        if ($a && HH\Lib\Str\length($a) > 5) {}
    }
}