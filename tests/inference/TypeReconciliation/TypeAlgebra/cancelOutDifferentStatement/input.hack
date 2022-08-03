function edit(?string $a, ?string $b): string {
    if (!$a && !$b) {
        return "";
    }

    if ($a && !$b) {
        return "";
    }

    return $b;
}