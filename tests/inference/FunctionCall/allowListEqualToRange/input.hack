function collectCommit(dict<arraykey, mixed> $one, dict<int, int> $two) : void {
    if ($one && array_values($one) === array_values($two)) {}
}