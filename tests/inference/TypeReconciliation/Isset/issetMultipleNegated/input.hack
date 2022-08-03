function foo(?string $a, ?string $b): string {
    if (!isset($a, $b)) {
        return "";
    }
    return $a . $b;
}