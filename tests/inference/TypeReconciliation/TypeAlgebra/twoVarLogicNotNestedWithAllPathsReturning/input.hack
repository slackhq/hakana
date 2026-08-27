function foo(?string $a, ?string $b): string {
    if ($a is null && $b is null) {
        return "bad";
    } else {
        if ($a is null) {
            return $b;
        } else {
            return $a;
        }
    }
}
