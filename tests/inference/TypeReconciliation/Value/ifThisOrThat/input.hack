$foo = "bar";

if (rand(0, 1) !== 0) {
    $foo = "bat";
} else if (rand(0, 1) !== 0) {
    $foo = "baz";
}

if ($foo === "baz" || $foo === "bar") {}
