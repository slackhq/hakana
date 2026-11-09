function takes_string(string $s): string {
    return $s;
}

function foo(?string $a, ?string $b, ?string $c): string {
    if (isset($a, $b, $c)) {
        return takes_string($a).takes_string($b).takes_string($c);
    }

    return '';
}

function bar(?string $a, ?string $b): string {
    if (!isset($a, $b)) {
        return '';
    }

    return takes_string($a).takes_string($b);
}
