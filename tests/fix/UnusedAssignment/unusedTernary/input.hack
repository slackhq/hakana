function foo(dict<string, int> $d): void {
    $a = isset($d['a']) ? $d['a'] : 4;
    $b = $d['b'] ?? 6;
}