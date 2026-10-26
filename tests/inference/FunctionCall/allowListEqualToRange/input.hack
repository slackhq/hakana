function collectCommit(dict<arraykey, mixed> $one, dict<int, int> $two) : void {
    if (!C\is_empty($one) && array_values($one) === array_values($two)) {}
}