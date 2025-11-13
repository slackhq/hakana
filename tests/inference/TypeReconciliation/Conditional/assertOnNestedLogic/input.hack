function foo(?string $a) : void {
    if (($a && rand(0, 1)) || rand(0, 1)) {
        if ($a && HH\Lib\Str\length($a) > 5) {}
    }
}