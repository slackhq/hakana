function takesString(string $s): void {}

function foo(?string $a, ?string $b): void {
    if ($a !== null || $b !== null) {
        if ($a !== null) {
            $c = $a;
        } else {
            $c = $b;
        }

        takesString($c);
    }
}