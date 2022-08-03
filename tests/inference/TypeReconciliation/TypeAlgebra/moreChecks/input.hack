function foo(?string $b, ?int $c): arraykey {
    if ($b === null && $c === null) {
        throw new Exception("bad");
    }

    if ($b !== null && $c !== null) {
        return rand(0, 1) ? $b : $c;
    }

    if ($b !== null) {
        return $b;
    }

    return $c;
}