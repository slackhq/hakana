function foo(?string $a, ?string $b): string {
    if (!(isset($a, $b) && rand(0, 1))) {
        return "";
    }
    return $a . $b;
}