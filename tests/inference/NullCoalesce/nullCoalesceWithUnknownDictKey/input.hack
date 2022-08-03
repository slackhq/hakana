function foo(string $s, dict<string, string> $arr): void {
    $a = $arr[$s] ?? null;
    hakana_expect_type<?string>($a);
}