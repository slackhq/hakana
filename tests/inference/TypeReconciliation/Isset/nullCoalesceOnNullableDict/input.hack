function foo(dict<int, ?int> $dict, shape('id' => int, ...) $row): int {
    return $dict[$row['id']] ?? 0;
}