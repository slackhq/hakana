$foo = "bar";

if (rand(0, 1)) {
    $foo = "bat";
} else if (rand(0, 1)) {
    $foo = "baz";
}

if ($foo === "baz" || $foo === "bar") {}