function checkbox(vec_or_dict $options = dict[]) : void {
    if ($options["a"]) {}

    unset($options["a"], $options["b"]);
}