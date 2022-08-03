function propertyInUse(dict<int, int> $tokens, int $i): bool {
    if ($tokens[$i] !== 1) {
        return false;
    }
    $i += 1;
    if ($tokens[$i] !== 2) {}
    return false;
}