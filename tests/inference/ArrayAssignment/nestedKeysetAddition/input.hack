function foo(int $a, vec<string> $strs): dict<string, keyset<string>> {
    $arr = dict[];

    foreach ($strs as $str) {
        $arr[$str] = keyset[];
        $arr[$str][] = $a;
    }

    return $arr;
}
