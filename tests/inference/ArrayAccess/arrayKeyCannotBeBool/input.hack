function foo(dict<arraykey, string> $arr) : void {
    if (!$arr) {
        return;
    }

    foreach ($arr as $i => $_) {}

    if ($i is bool) {}
}