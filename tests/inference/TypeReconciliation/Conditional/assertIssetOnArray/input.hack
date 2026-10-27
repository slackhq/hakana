function foo(bool $c, vec_or_dict $arr) : void {
    if ($c && !C\is_empty($arr) && isset($arr["b"]) && $arr["b"] === true) {
        return;
    }

    if ($c && rand(0, 1) !== 0) {}
}