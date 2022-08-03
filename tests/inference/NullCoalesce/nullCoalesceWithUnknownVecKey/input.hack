function foo(int $i): void {
    $arr = vec["hello", "goodbye"];
    $a = $arr[$i] ?? null;
    hakana_expect_type<?string>($a);
}
