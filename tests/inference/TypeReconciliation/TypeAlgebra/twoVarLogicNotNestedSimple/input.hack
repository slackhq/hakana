function foo(?string $a, ?string $b): string {
    if (!$a && !$b) return "bad";
    if (!$a) return $b;
    return $a;
}