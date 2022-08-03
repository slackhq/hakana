function takesString(string $s): void {}

function foo(?string $a, ?string $b, ?string $c): void {
    if ($a !== null || $b !== null || $c !== null) {
        if ($a !== null) {
            $d = $a;
        } else if ($b !== null) {
            $d = $b;
        } else {
            $d = $c;
        }

        takesString($d);
    }
}