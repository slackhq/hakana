$foo = "bar";

if (rand(0, 1) !== 0) {
    $foo = "bat";
} else if (rand(0, 1) !== 0) {
    $foo = "baz";
}

$bar = "bar";
$baz = "baz";

if ($foo === "bar") {}
if ($foo !== "bar") {}
if ($foo === "baz") {}
if ($foo === $bar) {}
if ($foo !== $bar) {}
if ($foo === $baz) {}
