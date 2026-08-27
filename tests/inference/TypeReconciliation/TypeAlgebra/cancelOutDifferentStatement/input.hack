function edit(?string $a, ?string $b): string {
    if (($a is null || $a === "") && ($b is null || $b === "")) {
        return "";
    }

    if (($a is nonnull && $a !== "") && ($b is null || $b === "")) {
        return "";
    }

    return $b ?? "";
}
