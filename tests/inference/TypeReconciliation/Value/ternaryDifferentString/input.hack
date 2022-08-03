$foo = rand(0, 1) ? "bar" : "bat";

if ($foo === "bar") {}

if ($foo !== "bar") {}

if (rand(0, 1)) {
    $foo = "baz";
}

if ($foo === "baz") {}

if ($foo !== "bat") {}