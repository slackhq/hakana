function foo(dict<arraykey, string> $arr) : void {
    if (C\is_empty($arr)) {
        return;
    }

    foreach ($arr as $i => $_) {}

    if ($i is bool) {}
}