function foo(dict<string, shape('a' => ?int)> $shape, shape('id' => string) $t): int {
    return $shape[$t['id']]['a'] as nonnull;
}