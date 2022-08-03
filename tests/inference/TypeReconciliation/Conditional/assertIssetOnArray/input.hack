function foo(bool $c, vec_or_dict $arr) : void {
    if ($c && $arr && isset($arr["b"]) && $arr["b"]) {
        return;
    }

    if ($c && rand(0, 1)) {}
}