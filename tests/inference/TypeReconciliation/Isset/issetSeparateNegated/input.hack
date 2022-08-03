function foo(?string $a, ?string $b): string {
    if (!isset($a) || !isset($b)) {
        return "";
    }
    return $a . $b;
}