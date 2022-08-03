function edit(?string $a, ?string $b): string {
    if ((!$a && !$b) || ($a && !$b)) {
        return "";
    }

    return $b;
}