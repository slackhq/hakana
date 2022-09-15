function foo(dict<string, dict<string, mixed>> $arrs) {
    foreach ($arrs as $arr) {
        $a = $arr['a'] ?? null;

        if ($a is nonnull) {
            $arr['a'] = $a;
        }

        $b = $arr['b'] ?? null;

        if ($b is nonnull) {

        }
    }
}