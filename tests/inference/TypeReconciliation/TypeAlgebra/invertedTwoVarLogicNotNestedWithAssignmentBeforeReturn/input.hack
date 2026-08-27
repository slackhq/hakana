function foo(?string $a, ?string $b): string {
    if (($a is nonnull && $a !== "") || ($b is nonnull && $b !== "")) {
        // do nothing
    } else {
        $a = 5;
        return "bad";
    }

    if ($a is null || $a === "") {
        return $b ?? "";
    }
    return $a;
}
