function foo(?string $a, ?string $b): string {
    if ($a is null && $b is null) {
        $a = 5;
        return "bad";
    }

    if ($a is null) {
        $a = 7;
        return $b;
    }

    return $a;
}
