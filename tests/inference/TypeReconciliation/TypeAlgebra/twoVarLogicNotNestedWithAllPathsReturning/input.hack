function foo(?string $a, ?string $b): string {
    if (!$a && !$b) {
        return "bad";
    } else {
        if (!$a) {
            return $b;
        } else {
            return $a;
        }
    }
}