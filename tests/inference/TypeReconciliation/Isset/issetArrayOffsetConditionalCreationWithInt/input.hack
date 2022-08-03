function foo(dict<int, string> $arr) : string {
    if (!isset($arr[0])) {
        $arr[0] = "hello";
    }

    return $arr[0];
}