function foo(?string $a, ?string $b): string {
    if ($a is null && $b is null) return "bad";
    if ($a is null) return $b;
    return $a;
}
