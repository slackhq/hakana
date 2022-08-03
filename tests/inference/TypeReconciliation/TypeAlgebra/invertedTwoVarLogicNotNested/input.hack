function foo(?string $a, ?string $b): string {
    if ($a || $b) {
        // do nothing
    } else {
        return "bad";
    }

    if (!$a) {
        return $b;
    }
    return $a;
}