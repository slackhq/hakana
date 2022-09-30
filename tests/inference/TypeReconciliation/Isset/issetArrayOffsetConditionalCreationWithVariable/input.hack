function foo(dict<int, string> $arr, int $b) : string {
    if (!isset($arr[$b])) {
        $arr[$b] = "hello";
    }

    return $arr[$b];
}