function foo(?string $a): string {
    $b = ($a ?? null) as nonnull;
    return $b;
}