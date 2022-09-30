function checkbox(dict<string, mixed> $options = dict[]) : void {
    if ($options["a"]) {}

    unset($options["a"], $options["b"]);
}