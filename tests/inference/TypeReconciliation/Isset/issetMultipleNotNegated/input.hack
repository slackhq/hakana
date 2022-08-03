function foo(?string $a, ?string $b): string {
    if (isset($a, $b)) {
        return $a . $b;
    }

    return "";
}