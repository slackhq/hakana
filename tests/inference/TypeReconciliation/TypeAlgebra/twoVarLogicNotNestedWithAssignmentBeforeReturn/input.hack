function foo(?string $a, ?string $b): string {
    if (!$a && !$b) {
        $a = 5;
        return "bad";
    }

    if (!$a) {
        $a = 7;
        return $b;
    }

    return $a;
}