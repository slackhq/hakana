function foo(vec_or_dict $arr) : void {
    $arr["a"] = 1;

    foreach ($arr["b"] as $b) {}
}