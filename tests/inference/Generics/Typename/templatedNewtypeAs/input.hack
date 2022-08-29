function foo(stringkey_dict<string> $dict) {
    echo $dict["a"];
    $b = $dict["b"] ?? null;
    if ($b is nonnull) {
        echo $b;
    }

    foreach ($dict as $k => $v) {
        echo $k;
        echo $v;
    }
}